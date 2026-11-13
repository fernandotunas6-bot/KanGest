import { useQuery } from "@tanstack/react-query";
import { Navigate, Route, Routes } from "react-router-dom";
import { call } from "@/lib/api";
import { useSession } from "@/store/session";
import type { Sessao } from "@/lib/types";
import AppShell from "@/components/layout/AppShell";
import Setup from "@/pages/Setup";
import Login from "@/pages/Login";
import Dashboard from "@/pages/Dashboard";
import Pos from "@/pages/Pos";
import Artigos from "@/pages/Artigos";
import Entidades from "@/pages/Entidades";
import Documentos from "@/pages/Documentos";
import NovoDocumento from "@/pages/NovoDocumento";
import Recibo from "@/pages/Recibo";
import Stock from "@/pages/Stock";
import Caixa from "@/pages/Caixa";
import Relatorios from "@/pages/Relatorios";
import Configuracoes from "@/pages/Configuracoes";
import Tesouraria from "@/pages/Tesouraria";
import TesourariaConfig from "@/pages/TesourariaConfig";
import Contas from "@/pages/Contas";
import Inventario from "@/pages/Inventario";
import Importar from "@/pages/Importar";
import Lotes from "@/pages/Lotes";
import Moedas from "@/pages/Moedas";
import Series from "@/pages/Series";
import RelatoriosStock from "@/pages/RelatoriosStock";
import Rastreio from "@/pages/Rastreio";
import MapaFornecedor from "@/pages/MapaFornecedor";
import AvisoVencimento from "@/pages/AvisoVencimento";
import Estorno from "@/pages/Estorno";
import Adiantamento from "@/pages/Adiantamento";
import Etiquetas from "@/pages/Etiquetas";
import Sobre from "@/pages/Sobre";
import Parametrizacao from "@/pages/Parametrizacao";
import FacturacaoElectronica from "@/pages/FacturacaoElectronica";
import RecalculoStock from "@/pages/RecalculoStock";
import Funcionarios from "@/pages/rh/Funcionarios";
import Salarios from "@/pages/rh/Salarios";
import FeriasFaltas from "@/pages/rh/Ferias";
import ConfigRH from "@/pages/rh/ConfigRH";
import Honorarios from "@/pages/rh/Honorarios";
import Viaturas from "@/pages/oficina/Viaturas";
import Ordens from "@/pages/oficina/Ordens";
import Utilizadores from "@/pages/Utilizadores";
import Licenca from "@/pages/Licenca";
import { EcraBloqueado, MudarSenhaObrigatoria, usePolitica, useInatividade } from "@/components/Sessao";

export default function App() {
  const { sessao, setSessao, pode } = useSession();
  const politica = usePolitica();
  useInatividade(politica.data?.inatividade_min ?? 0, !!sessao && !sessao.bloqueada && !sessao.mudar_senha);
  const estado = useQuery({
    queryKey: ["setup"],
    queryFn: async () => {
      const e = await call<{ inicializado: boolean; empresa: string | null }>("setup.estado");
      const s = await call<Sessao | null>("auth.sessao");
      setSessao(s);
      return e;
    },
  });

  if (estado.isLoading) {
    return <div className="grid h-full place-items-center text-ink-3">A carregar…</div>;
  }
  if (estado.error) {
    return (
      <div className="grid h-full place-items-center p-8 text-center">
        <div>
          <p className="text-lg font-semibold">Não foi possível iniciar</p>
          <p className="mt-1 text-ink-3">{(estado.error as Error).message}</p>
        </div>
      </div>
    );
  }
  if (!estado.data?.inicializado) return <Setup onDone={() => estado.refetch()} />;
  if (!sessao) return <Login empresa={estado.data.empresa} />;
  if (sessao.mudar_senha) return <MudarSenhaObrigatoria />;

  return (
    <>
    <Routes>
      <Route path="/pos" element={<Pos />} />
      <Route element={<AppShell />}>
        <Route path="/" element={pode("GESTOR") ? <Dashboard /> : <Navigate to="/pos" replace />} />
        <Route path="/artigos" element={<Artigos />} />
        <Route path="/clientes" element={<Entidades tipo="C" />} />
        <Route path="/fornecedores" element={<Entidades tipo="F" />} />
        <Route path="/documentos" element={<Documentos />} />
        <Route path="/documentos/novo/:tipo" element={<NovoDocumento />} />
        <Route path="/recibos/novo" element={<Recibo tipo="RC" />} />
        <Route path="/pagamentos/novo" element={<Recibo tipo="VRC" />} />
        <Route path="/adiantamentos/novo" element={<Adiantamento />} />
        <Route path="/recibos/estorno" element={<Estorno tipo="RC" />} />
        <Route path="/pagamentos/estorno" element={<Estorno tipo="VRC" />} />
        <Route path="/stock" element={<Stock />} />
        <Route path="/inventario" element={<Inventario />} />
        <Route path="/tesouraria" element={<Tesouraria />} />
        <Route path="/tesouraria/configuracao" element={<TesourariaConfig />} />
        <Route path="/contas/:tipo" element={<Contas />} />
        <Route path="/avisos" element={<AvisoVencimento />} />
        <Route path="/importar" element={<Importar />} />
        <Route path="/caixa" element={<Caixa />} />
        <Route path="/relatorios" element={<Relatorios />} />
        <Route path="/relatorios/stock" element={<RelatoriosStock />} />
        <Route path="/relatorios/rastreio" element={<Rastreio />} />
        <Route path="/relatorios/fornecedores" element={<MapaFornecedor />} />
        <Route path="/configuracoes" element={<Configuracoes />} />
        <Route path="/lotes" element={<Lotes />} />
        <Route path="/etiquetas" element={<Etiquetas />} />
        <Route path="/sobre" element={<Sobre />} />
        <Route path="/parametrizacao" element={<Parametrizacao />} />
        <Route path="/facturacao-electronica" element={<FacturacaoElectronica />} />
        <Route path="/stock/recalculo" element={<RecalculoStock />} />
        <Route path="/moedas" element={<Moedas />} />
        <Route path="/series" element={<Series />} />
        <Route path="/rh/funcionarios" element={<Funcionarios />} />
        <Route path="/rh/salarios" element={<Salarios />} />
        <Route path="/rh/ferias" element={<FeriasFaltas />} />
        <Route path="/rh/configuracao" element={<ConfigRH />} />
        <Route path="/rh/honorarios" element={<Honorarios />} />
        <Route path="/oficina/viaturas" element={<Viaturas />} />
        <Route path="/oficina/ordens" element={<Ordens />} />
        <Route path="/utilizadores" element={<Utilizadores />} />
        <Route path="/licenca" element={<Licenca />} />
      </Route>
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
    {sessao.bloqueada && <EcraBloqueado />}
    </>
  );
}
