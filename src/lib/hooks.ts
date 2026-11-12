import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { call } from "./api";
import type { Tabelas } from "./types";

export function useTabelas() {
  return useQuery({ queryKey: ["tabelas"], queryFn: () => call<Tabelas>("tabelas"), staleTime: 60_000 });
}

/** Talão de 58 mm no posto actual (por omissão 80 mm). */
export function useTalao58() {
  return useTabelas().data?.posto?.opcoes?.talao_mm === 58;
}

export function useDebounce<T>(v: T, ms = 250) {
  const [d, setD] = useState(v);
  useEffect(() => {
    const t = setTimeout(() => setD(v), ms);
    return () => clearTimeout(t);
  }, [v, ms]);
  return d;
}
