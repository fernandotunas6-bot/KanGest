mod atualizacao;
mod auth;
mod caixa;
mod catalogo;
mod contas;
mod documentos;
mod entidades;
pub mod fe;
mod impressao;
pub mod importar;
pub mod licenca;
mod oficina;
mod parametros;
mod posto;
mod rh;
mod tesouraria;
mod relatorios;
mod saft;
mod series;
mod setup;
pub mod sistema;
mod suspensos;
#[cfg(test)]
mod tests;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

fn p<T: DeserializeOwned>(v: Value) -> AppResult<T> {
    // Aceita `null` como objecto vazio para métodos sem parâmetros obrigatórios.
    let v = if v.is_null() { Value::Object(Default::default()) } else { v };
    Ok(serde_json::from_value(v)?)
}

pub async fn dispatch(st: &AppState, metodo: &str, params: Value) -> AppResult<Value> {
    // Senha provisória por mudar ou sessão bloqueada: só se pode mudar a senha, retomar ou sair.
    let livre = matches!(
        metodo,
        "setup.estado" | "setup.inicializar" | "auth.login" | "auth.loginPin" | "auth.logout" | "auth.sessao" | "auth.alterarSenha" | "auth.desbloquear" | "seguranca.politica"
    );
    if !livre {
        if let Some(s) = st.sessao_opcional() {
            if s.bloqueada {
                return Err(AppError::msg("A sessão está bloqueada. Introduza a sua senha ou PIN para continuar."));
            }
            if s.mudar_senha {
                return Err(AppError::msg("Mude a sua senha provisória antes de continuar."));
            }
        }
    }
    match metodo {
        // ---- públicos ----
        "setup.estado" => setup::estado(st).await,
        "setup.inicializar" => setup::inicializar(st, p(params)?).await,
        "auth.login" => auth::login(st, p(params)?).await,
        "auth.logout" => auth::logout(st).await,
        "auth.loginPin" => auth::login_pin(st, p(params)?).await,
        "auth.bloquear" => auth::bloquear(st).await,
        "auth.desbloquear" => auth::desbloquear(st, p(params)?).await,
        "seguranca.politica" => auth::politica_publica(st).await,
        "auth.sessao" => Ok(serde_json::to_value(st.sessao_opcional())?),

        // ---- utilizadores / empresa ----
        "utilizadores.listar" => auth::listar(st).await,
        "utilizadores.guardar" => auth::guardar(st, p(params)?).await,
        "utilizadores.redefinirSenha" => auth::redefinir_senha(st, p(params)?).await,
        "utilizadores.desbloquear" => auth::desbloquear_conta(st, p(params)?).await,
        "perfis.listar" => auth::perfis_listar(st).await,
        "perfis.guardar" => auth::perfis_guardar(st, p(params)?).await,
        "perfis.eliminar" => auth::perfis_eliminar(st, p(params)?).await,
        "seguranca.obter" => auth::seguranca_obter(st).await,
        "seguranca.guardar" => auth::seguranca_guardar(st, p(params)?).await,
        "acessos.listar" => auth::acessos_listar(st, p(params)?).await,
        "auditoria.listar" => auth::auditoria_listar(st, p(params)?).await,
        "auth.alterarSenha" => auth::alterar_senha(st, p(params)?).await,
        "auth.autorizar" => auth::autorizar(st, p(params)?).await,
        "empresa.obter" => setup::empresa_obter(st).await,
        "empresa.guardar" => setup::empresa_guardar(st, p(params)?).await,
        "agt.info" => setup::agt_info(st).await,
        "agt.guardarCertificado" => setup::agt_guardar(st, p(params)?).await,
        "agt.importarChave" => setup::agt_importar_chave(st, p(params)?).await,

        // ---- importação do KIGEST (SQL Server) ----
        "importar.analisar" => importar::analisar(st, p(params)?).await,
        "importar.executar" => importar::executar(st, p(params)?).await,
        "importar.analisarFicheiro" => importar::analisar_ficheiro(st, p(params)?).await,
        "importar.executarFicheiro" => importar::executar_ficheiro(st, p(params)?).await,
        "importar.ficheiros" => importar::ficheiros(st).await,

        // ---- tesouraria / conta corrente ----
        "tesouraria.contas" => tesouraria::contas(st).await,
        "tesouraria.contaGuardar" => tesouraria::conta_guardar(st, p(params)?).await,
        "tesouraria.centros" => tesouraria::centros(st).await,
        "tesouraria.centroGuardar" => tesouraria::centro_guardar(st, p(params)?).await,
        "tesouraria.fluxo" => tesouraria::fluxo(st, p(params)?).await,
        "tesouraria.resumoMensal" => tesouraria::resumo_mensal(st, p(params)?).await,
        "tesouraria.porCentro" => tesouraria::por_centro(st, p(params)?).await,
        "tesouraria.movimento" => tesouraria::movimento(st, p(params)?).await,
        "tesouraria.transferencia" => tesouraria::transferencia(st, p(params)?).await,
        "tesouraria.anular" => tesouraria::anular(st, p(params)?).await,
        "contas.pendentes" => contas::pendentes(st, p(params)?).await,
        "contas.extrato" => contas::extrato(st, p(params)?).await,
        "contas.creditos" => contas::creditos(st, p(params)?).await,
        "contas.devolver" => contas::devolver(st, p(params)?).await,
        "contas.regularizar" => contas::regularizar(st, p(params)?).await,
        "contas.regularizacaoAnular" => contas::regularizacao_anular(st, p(params)?).await,
        "contas.regularizacoes" => contas::regularizacoes(st, p(params)?).await,
        "contas.vencidos" => contas::vencidos(st, p(params)?).await,

        // ---- parametrização de documentos / opções de venda / definição de facturas ----
        "parametros.obter" => parametros::obter(st).await,
        "parametros.tipoGuardar" => parametros::tipo_guardar(st, p(params)?).await,
        "parametros.modoGuardar" => parametros::modo_guardar(st, p(params)?).await,
        "parametros.opcoesGuardar" => parametros::opcoes_guardar(st, params).await,

        // ---- séries de documentos ----
        "series.listar" => series::listar(st, p(params)?).await,
        "series.guardar" => series::guardar(st, p(params)?).await,
        "series.estado" => series::estado(st, p(params)?).await,
        "series.eliminar" => series::eliminar(st, p(params)?).await,

        // ---- postos ----
        "licenca.estado" => licenca::estado(st).await,
        "licenca.activar" => licenca::activar(st, p(params)?).await,
        "licenca.abrirSite" => licenca::abrir_site(p(params)?).await,
        "licenca.registar" => licenca::registar(st, p(params)?).await,
        "licenca.confirmar" => licenca::confirmar(st, p(params)?).await,
        "licenca.sincronizar" => licenca::sincronizar(st, p(params)?).await,
        "posto.obter" => posto::obter(st).await,
        "posto.guardar" => posto::guardar(st, p(params)?).await,
        "posto.selecionar" => posto::selecionar(st, p(params)?).await,

        // ---- postos em rede (servidor / cliente) ----
        "rede.estado" => crate::rede::estado(st).await,
        "rede.guardar" => crate::rede::guardar(st, p(params)?).await,
        "rede.testar" => crate::rede::testar(st).await,

        // ---- impressão térmica (ESC/POS) e gaveta ----
        "impressao.estado" => impressao::estado(st).await,
        "impressao.talao" => impressao::talao(st, p(params)?).await,
        "impressao.gaveta" => impressao::gaveta(st, p(params)?).await,
        "impressao.teste" => impressao::teste(st).await,
        "impressao.visor" => impressao::visor(st, p(params)?).await,
        "impressao.visorTeste" => impressao::visor_teste(st).await,

        // ---- catálogo ----
        "tabelas" => catalogo::tabelas(st).await,
        "tabela.guardar" => catalogo::tabela_guardar(st, p(params)?).await,
        "artigos.listar" => catalogo::artigos_listar(st, p(params)?).await,
        "artigos.obter" => catalogo::artigos_obter(st, p(params)?).await,
        "artigos.porIds" => catalogo::artigos_por_ids(st, p(params)?).await,
        "artigos.codigo" => catalogo::artigos_codigo(st, p(params)?).await,
        "artigos.guardar" => catalogo::artigos_guardar(st, p(params)?).await,
        "stock.listar" => catalogo::stock_listar(st, p(params)?).await,
        "stock.movimentos" => catalogo::stock_movimentos(st, p(params)?).await,
        "stock.recalcular" => catalogo::stock_recalcular(st, p(params)?).await,
        "artigos.componentes" => catalogo::artigos_componentes(st, p(params)?).await,
        "lotes.listar" => catalogo::lotes_listar(st, p(params)?).await,
        "moedas.listar" => catalogo::moedas(st).await,
        "moedas.guardar" => catalogo::moeda_guardar(st, p(params)?).await,

        // ---- entidades ----
        "entidades.listar" => entidades::listar(st, p(params)?).await,
        "entidades.obter" => entidades::obter(st, p(params)?).await,
        "entidades.guardar" => entidades::guardar(st, p(params)?).await,
        "entidades.pendentes" => entidades::pendentes(st, p(params)?).await,
        "entidades.enderecos" => entidades::enderecos(st, p(params)?).await,
        "entidades.enderecoGuardar" => entidades::endereco_guardar(st, p(params)?).await,

        // ---- documentos ----
        "documentos.emitir" => documentos::emitir(st, p(params)?).await,
        "documentos.listar" => documentos::listar(st, p(params)?).await,
        "documentos.obter" => documentos::obter(st, p(params)?).await,
        "documentos.anular" => documentos::anular(st, p(params)?).await,
        "documentos.auxiliares" => documentos::auxiliares(st).await,

        // ---- POS: suspensos ----
        "suspensos.guardar" => suspensos::guardar(st, p(params)?).await,
        "suspensos.listar" => suspensos::listar(st).await,
        "suspensos.abrir" => suspensos::abrir(st, p(params)?).await,
        "suspensos.eliminar" => suspensos::eliminar(st, p(params)?).await,

        // ---- caixa ----
        "caixa.estado" => caixa::estado(st).await,
        "caixa.obter" => caixa::obter(st, p(params)?).await,
        "caixa.sessoes" => caixa::sessoes(st).await,
        "caixa.abrir" => caixa::abrir(st, p(params)?).await,
        "caixa.movimento" => caixa::movimento(st, p(params)?).await,
        "caixa.fechar" => caixa::fechar(st, p(params)?).await,

        // ---- recursos humanos ----
        "rh.tabelas" => rh::tabelas(st).await,
        "rh.tabelaGuardar" => rh::tabela_guardar(st, p(params)?).await,
        "rh.configGuardar" => rh::config_guardar(st, p(params)?).await,
        "rh.funcionarios" => rh::funcionarios(st, p(params)?).await,
        "rh.agregados" => rh::agregados(st).await,
        "rh.funcionario" => rh::funcionario(st, p(params)?).await,
        "rh.funcionarioGuardar" => rh::funcionario_guardar(st, params).await,
        "rh.alteracoes" => rh::alteracoes(st, p(params)?).await,
        "rh.alteracaoGuardar" => rh::alteracao_guardar(st, p(params)?).await,
        "rh.alteracaoEliminar" => rh::alteracao_eliminar(st, p(params)?).await,
        "rh.adiantamento" => rh::adiantamento(st, p(params)?).await,
        "rh.simular" => rh::simular(st, p(params)?).await,
        "rh.processar" => rh::processar(st, p(params)?).await,
        "rh.processamentos" => rh::processamentos(st).await,
        "rh.recibos" => rh::recibos(st, p(params)?).await,
        "rh.anular" => rh::anular(st, p(params)?).await,
        "rh.pagar" => rh::pagar(st, p(params)?).await,
        "rh.mapa" => rh::mapa(st, p(params)?).await,
        "rh.ferias" => rh::ferias(st, p(params)?).await,
        "rh.feriasGuardar" => rh::ferias_guardar(st, p(params)?).await,
        "rh.vendedores" => rh::vendedores(st).await,
        "rh.honorarios" => rh::honorarios(st, p(params)?).await,
        "rh.honorarioProcessar" => rh::honorario_processar(st, p(params)?).await,
        "rh.honorarioAnular" => rh::honorario_anular(st, p(params)?).await,
        "rh.honorarioPagar" => rh::honorario_pagar(st, p(params)?).await,
        "rh.prestacoes" => rh::prestacoes(st, p(params)?).await,
        "rh.prestacaoGuardar" => rh::prestacao_guardar(st, p(params)?).await,
        "rh.prestacaoEliminar" => rh::prestacao_eliminar(st, p(params)?).await,

        // ---- oficina ----
        "oficina.viaturas" => oficina::viaturas(st, p(params)?).await,
        "oficina.viaturaGuardar" => oficina::viatura_guardar(st, p(params)?).await,
        "oficina.sugestoes" => oficina::sugestoes(st).await,
        "oficina.ordens" => oficina::ordens(st, p(params)?).await,
        "oficina.ordem" => oficina::ordem(st, p(params)?).await,
        "oficina.ordemGuardar" => oficina::ordem_guardar(st, p(params)?).await,
        "oficina.associar" => oficina::associar(st, p(params)?).await,

        // ---- relatórios / sistema ----
        "dashboard" => relatorios::dashboard(st).await,
        "relatorios.vendas" => relatorios::vendas(st, p(params)?).await,
        "relatorios.sintese" => relatorios::sintese(st, p(params)?).await,
        "relatorios.irtC" => relatorios::irt_c_obter(st).await,
        "relatorios.irtCGuardar" => relatorios::irt_c_guardar(st, p(params)?).await,
        "relatorios.stock" => relatorios::stock(st, p(params)?).await,
        "relatorios.rastreio" => relatorios::rastreio(st, p(params)?).await,
        "relatorios.fornecedores" => relatorios::fornecedores(st, p(params)?).await,
        "saft.exportar" => saft::exportar(st, p(params)?).await,
        "saft.exportarCompras" => saft::exportar_compras(st, p(params)?).await,
        "saft.exportarEstado" => saft::exportar_estado(st, p(params)?).await,
        "sistema.backup" => sistema::backup(st).await,
        "sistema.copias" => sistema::copias(st).await,
        "sistema.info" => sistema::info(st).await,
        "fe.estado" => fe::estado(st).await,
        "fe.guardar" => fe::guardar(st, p(params)?).await,
        "fe.processar" => fe::processar(st).await,
        "fe.envios" => fe::envios(st, p(params)?).await,
        "fe.reenviar" => fe::reenviar(st, p(params)?).await,
        "fe.corrigir" => fe::corrigir(st, p(params)?).await,
        "fe.solicitarSerie" => fe::solicitar_serie(st, p(params)?).await,
        "fe.listarSeries" => fe::listar_series(st).await,
        "atualizacao.estado" => atualizacao::estado(st).await,
        "atualizacao.guardar" => atualizacao::guardar(st, p(params)?).await,
        "atualizacao.verificar" => atualizacao::verificar(st).await,
        "atualizacao.descarregar" => atualizacao::descarregar(st).await,
        "atualizacao.instalar" => atualizacao::instalar(st, p(params)?).await,
        "sistema.exportar" => sistema::exportar(st, p(params)?).await,

        _ => Err(AppError::msg(format!("Método desconhecido: {metodo}"))),
    }
}
