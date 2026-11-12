import type { ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { call } from "@/lib/api";
import type { Empresa } from "@/lib/types";

export function useEmpresa() {
  return useQuery({ queryKey: ["empresa"], queryFn: () => call<Empresa>("empresa.obter"), staleTime: 300_000 }).data;
}

/** Cabeçalho A4 com os dados da empresa (OS, recibos de salário, mapas). */
export function CabecalhoEmpresa({ empresa, titulo, numero, children }: { empresa?: Empresa; titulo: string; numero?: string; children?: ReactNode }) {
  const e = empresa;
  return (
    <div className="flex items-start justify-between border-b-2 border-black pb-3">
      <div className="flex items-start gap-4">
        {e?.logo && <img src={e.logo} alt="" className="max-h-16 max-w-[40mm] object-contain" />}
        <div>
          <div className="text-[16px] font-bold">{e?.designacao_comercial || e?.nome}</div>
          {e?.designacao_comercial && <div>{e.nome}</div>}
          <div>NIF: {e?.nif}</div>
          <div>{[e?.endereco, e?.localidade, e?.provincia].filter(Boolean).join(", ")}</div>
          <div>{[e?.telefone && `Tel: ${e.telefone}`, e?.email].filter(Boolean).join(" · ")}</div>
        </div>
      </div>
      <div className="text-right">
        <div className="text-[15px] font-bold uppercase">{titulo}</div>
        {numero && <div className="text-[14px] font-semibold">{numero}</div>}
        {children}
      </div>
    </div>
  );
}
