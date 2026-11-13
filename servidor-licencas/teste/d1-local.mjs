// Imitação mínima do Cloudflare D1 sobre node:sqlite, para testar o Worker sem rede.
import { DatabaseSync } from "node:sqlite";

class Instrucao {
  constructor(db, sql, args = []) { this.db = db; this.sql = sql; this.args = args; }
  bind(...args) { return new Instrucao(this.db, this.sql, args.map((a) => (a === undefined ? null : typeof a === "boolean" ? Number(a) : a))); }
  _st() { return this.db.prepare(this.sql); }
  async first() { return this._st().get(...this.args) ?? null; }
  async all() { return { results: this._st().all(...this.args), success: true }; }
  async run() { const r = this._st().run(...this.args); return { success: true, meta: { changes: Number(r.changes) } }; }
}

export function d1Local(schema) {
  const db = new DatabaseSync(":memory:");
  db.exec(schema);
  return {
    prepare: (sql) => new Instrucao(db, sql),
    async batch(lista) {
      db.exec("BEGIN");
      try {
        const r = [];
        for (const i of lista) r.push(/^\s*(SELECT|WITH)/i.test(i.sql) ? await i.all() : await i.run());
        db.exec("COMMIT");
        return r;
      } catch (e) { db.exec("ROLLBACK"); throw e; }
    },
    exec: async (sql) => db.exec(sql),
  };
}
