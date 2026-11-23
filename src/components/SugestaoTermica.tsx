// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Sugestão no POS: há uma impressora de talões instalada no sistema, mas o posto imprime pela janela de impressão
// (HTML). Com a impressão directa (ESC/POS) o talão sai sem janela, com o tamanho dele, a impressora corta o papel e
// abre a gaveta. Um clique grava o posto e imprime um talão de teste; «Agora não» não volta a perguntar neste posto.
import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Printer } from "lucide-react";
import { call, NA_APP } from "@/lib/api";
import type { EscPos, Posto, PostoOpcoes } from "@/lib/types";
import { Button, cn } from "@/components/ui";

// nomes habituais das impressoras de talões (POS80, POS-58, XP-80C, EPSON TM-T20II Receipt, Star TSP100…)
const TALOES =
  /\b(?:pos|xp|tm-?[a-z]?|tsp|rp|srp|ct-?s)[- ]?\d{2,}|\b(?:receipt|thermal|t[eé]rmica|tal[aã]o|bixolon|rongta|sewoo|gprinter|zjiang|xprinter)\b|\b(?:58|80)\s?mm\b/i;
const NAO_SAO = /pdf|xps|onenote|fax|anydesk|send to/i;

/** A primeira impressora do sistema com nome de impressora de talões. */
export function impressoraDeTaloes(nomes: string[]): string | null {
  return nomes.find((n) => TALOES.test(n) && !NAO_SAO.test(n)) ?? null;
}

export function SugestaoTermica({ posto, opcoes, pode, className }: { posto?: Posto; opcoes: PostoOpcoes; pode: boolean; className?: string }) {
  const qc = useQueryClient();
  const chave = `kg-sugestao-termica:${posto?.id ?? 0}`;
  const [fechada, setFechada] = useState(() => {
    try {
      return localStorage.getItem(chave) === "nao";
    } catch {
      return false;
    }
  });
  const [busy, setBusy] = useState(false);
  const ativa = NA_APP && pode && !!posto && !opcoes.escpos?.ativo && opcoes.formato === "ticket" && !fechada;
  const estado = useQuery({ queryKey: ["impressao"], queryFn: () => call<{ impressoras: string[] }>("impressao.estado"), enabled: ativa, staleTime: 300_000 });
  const nome = impressoraDeTaloes(estado.data?.impressoras ?? []);
  if (!ativa || !nome || !posto) return null;

  async function ligar() {
    if (!posto || !nome) return;
    setBusy(true);
    try {
      const mm = /58/.test(nome) ? 58 : 80;
      const escpos: EscPos = {
        ligacao: "fila",
        colunas: mm === 58 ? 32 : 42,
        cortar: true,
        acentos: true,
        gaveta: "numerario",
        pino: 2,
        ...(opcoes.escpos ?? {}),
        ativo: true,
        destino: nome,
      };
      await call("posto.guardar", { id: posto.id, descricao: posto.descricao, armazem_id: posto.armazem_id, serie_prefixo: posto.serie_prefixo, opcoes: { ...opcoes, talao_mm: mm, escpos } });
      qc.invalidateQueries({ queryKey: ["tabelas"] });
      qc.invalidateQueries({ queryKey: ["postos"] });
      qc.invalidateQueries({ queryKey: ["impressao"] });
      await call("impressao.teste");
      toast.success(`Os talões passam a sair directamente na «${nome}».`, {
        description: "Saiu um talão de teste. Se não saiu, ou os acentos saem trocados, ajuste em Configurar posto → Impressora térmica.",
        duration: 10000,
      });
    } catch (e) {
      toast.error((e as Error).message, { description: "Pode configurar a impressora em Configurar posto → Impressora térmica." });
    } finally {
      setBusy(false);
    }
  }

  function agoraNao() {
    try {
      localStorage.setItem(chave, "nao");
    } catch {
      /* sem armazenamento: volta a aparecer da próxima vez */
    }
    setFechada(true);
  }

  return (
    <div className={cn("flex flex-wrap items-center gap-x-3 gap-y-2 rounded-xl border border-brand/25 bg-brand-50 px-4 py-2.5 text-[13px]", className)}>
      <Printer className="size-4 shrink-0 text-brand" />
      <span className="min-w-0 flex-1">
        <b>«{nome}»</b> pode imprimir os talões directamente: sem a janela do Windows, sem papel a mais, corta o papel e abre a gaveta.
      </span>
      <Button size="sm" loading={busy} onClick={ligar}>
        Imprimir directamente
      </Button>
      <Button size="sm" variant="ghost" onClick={agoraNao}>
        Agora não
      </Button>
    </div>
  );
}
