// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// O Cloudflare D1 sobre Postgres: o servidor de licenças (servidor-licencas/src/worker.js) corre na função kangest do
// Supabase sem mudar uma linha. Ver docs/ARQUITETURA.md.
//
// - «?» e «?N» do SQLite passam a «$N» do Postgres (com a regra do SQLite para «?» depois de «?N»);
// - cada tabela ganha o seu esquema (clientes → empresas.clientes): não depende do search_path da ligação;
// - verdadeiro/falso vão como 1/0, e undefined e números não finitos como NULL, como no D1;
// - first/all/run/batch devolvem o que o D1 devolve; batch corre numa transacção.
// Sem dependências: o motor (postgres.js na função, PGlite nos testes) entra por d1Postgres().

/** Em que esquema está cada tabela (o mesmo da migração em supabase/migrations). */
export const ESQUEMAS = {
  empresas: ["clientes", "contas", "planos"],
  licencas: ["maquinas", "instalacoes", "licencas", "pagamentos", "codigos"],
  equipa: ["funcionarios", "lojas_geridas", "postos_geridos", "equipa_versoes", "equipas"],
  seguranca: ["sessoes_painel", "sessoes_conta", "codigos_conta", "segredos_maquina", "limites"],
  web: ["subdominios"],
};

const ESQUEMA_DE = new Map(Object.entries(ESQUEMAS).flatMap(([esquema, tabelas]) => tabelas.map((t) => [t, esquema])));
// o nome de uma tabela depois de FROM/JOIN/INTO/UPDATE, se ainda não tiver esquema
const TABELA = new RegExp(String.raw`\b(FROM|JOIN|INTO|UPDATE)(\s+)(${[...ESQUEMA_DE.keys()].join("|")})\b(?!\s*\.)`, "gi");
// cadeias entre plicas, identificadores entre aspas e comentários: ficam como estão
const LITERAL = /('(?:[^']|'')*'|"(?:[^"]|"")*"|--[^\n]*)/;

const convertidas = new Map();

/** SQL do D1 (SQLite) → SQL do Postgres. */
export function paraPostgres(sql) {
  let feita = convertidas.get(sql);
  if (feita !== undefined) return feita;
  let maior = 0;
  feita = sql
    .split(LITERAL)
    .map((parte, i) => {
      if (i % 2) return parte;
      return parte
        .replace(/\?(\d*)/g, (_, n) => {
          const k = n ? Number(n) : maior + 1;
          maior = Math.max(maior, k);
          return "$" + k;
        })
        .replace(TABELA, (_, palavra, espaco, tabela) => `${palavra}${espaco}${ESQUEMA_DE.get(tabela.toLowerCase())}.${tabela}`);
    })
    .join("");
  convertidas.set(sql, feita);
  return feita;
}

/** Um valor ligado com bind(), como o D1 o guarda. */
function valor(v) {
  if (v === undefined) return null;
  if (typeof v === "boolean") return v ? 1 : 0;
  if (typeof v === "number" && !Number.isFinite(v)) return null;
  return v;
}

const resultado = (r) => ({ success: true, results: r.linhas, meta: { changes: r.alteradas, rows_read: r.linhas.length } });

class Instrucao {
  constructor(base, sql, args = []) {
    this.base = base;
    this.sql = sql;
    this.args = args;
  }
  bind(...args) {
    return new Instrucao(this.base, this.sql, args.map(valor));
  }
  async all() {
    return resultado(await this.base.executar(paraPostgres(this.sql), this.args));
  }
  async first(coluna) {
    const linha = (await this.all()).results[0] ?? null;
    return coluna === undefined ? linha : (linha?.[coluna] ?? null);
  }
  async run() {
    return this.all();
  }
}

/**
 * Uma base com a API do D1 (prepare/bind/first/all/run/batch) sobre um motor Postgres:
 * - executar(sql, args) → { linhas, alteradas };
 * - transacao(fn) corre fn(executar) numa transacção (desfaz tudo se fn falhar).
 */
export function d1Postgres({ executar, transacao }) {
  const base = {
    executar,
    prepare: (sql) => new Instrucao(base, sql),
    batch: (lista) =>
      transacao(async (exec) => {
        const r = [];
        for (const i of lista) r.push(resultado(await exec(paraPostgres(i.sql), i.args)));
        return r;
      }),
  };
  return base;
}

/**
 * Opções do postgres.js na função: sem instruções preparadas (o pooler do Supabase em modo transacção não as guarda),
 * poucas ligações (cada instância da função tem as suas) e int8/numeric (COUNT, SUM, bigint) como números, como o D1.
 */
export const OPCOES_POSTGRES = {
  prepare: false,
  max: 3,
  idle_timeout: 20,
  connect_timeout: 10,
  fetch_types: false,
  types: {
    int8: { to: 20, from: [20], serialize: (x) => String(x), parse: (x) => Number(x) },
    numeric: { to: 1700, from: [1700], serialize: (x) => String(x), parse: (x) => Number(x) },
  },
};

/** O D1 sobre uma ligação do postgres.js (npm:postgres), criada com OPCOES_POSTGRES. */
export function d1SobrePostgresJs(sql) {
  const executor = (alvo) => async (texto, args) => {
    const r = await alvo.unsafe(texto, args);
    return { linhas: Array.from(r), alteradas: r.command === "SELECT" ? 0 : (r.count ?? 0) };
  };
  return d1Postgres({ executar: executor(sql), transacao: (fn) => sql.begin((tx) => fn(executor(tx))) });
}
