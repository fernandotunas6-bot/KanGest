// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

pub mod acesso_web;
mod aparencia;
mod auditoria;
mod dois_passos;
mod atualizacao;
mod avencas;
mod assistente;
mod auth;
// motor central de dados (construtor de relatórios, BI, qualidade dos dados)
mod dados;
mod caixa;
mod comissoes;
pub(crate) mod catalogo;
mod contas;
mod documentos;
mod entidades;
mod equipa_conta;
mod escola;
mod humano;
mod tv;
pub mod fe;
pub(crate) mod impressao;
pub mod importar;
mod importar_folha;
mod importar_saft;
mod integridade;
pub mod licenca;
mod oficina;
mod fidelizacao;
mod gestao;
mod inteligencia;
mod parametros;
mod partilha;
mod promocoes;
mod posto;
mod rh;
mod tesouraria;
mod relatorios;
pub mod resumo_email;
pub(crate) mod saft;
mod series;
mod setup;
pub mod sistema;
mod suspensos;
mod ficheiros;
pub mod fotos;
mod inventario;
mod ligacao;
mod modos;
mod quadro;
mod ramos;
#[cfg(test)]
mod tests;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Sim/não vindo do ecrã como `true`/`false` ou como na base (`1`/`0`, «1»/«0»): as fichas lidas da base e
/// devolvidas ao gravar trazem os números (ex.: o `estado` da entidade).
pub(crate) fn bool_flexivel<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
    use serde::Deserialize;
    Ok(match Option::<Value>::deserialize(d)? {
        None | Some(Value::Null) => None,
        Some(Value::Bool(b)) => Some(b),
        Some(Value::Number(n)) => Some(n.as_f64().unwrap_or(0.0) != 0.0),
        Some(Value::String(s)) => Some(matches!(s.trim().to_lowercase().as_str(), "1" | "true" | "sim")),
        Some(_) => return Err(serde::de::Error::custom("esperado sim/não")),
    })
}

fn p<T: DeserializeOwned>(v: Value) -> AppResult<T> {
    // Aceita `null` como objecto vazio para métodos sem parâmetros obrigatórios.
    let v = if v.is_null() { Value::Object(Default::default()) } else { v };
    Ok(serde_json::from_value(v)?)
}

/// Entrada de todos os pedidos. Nos métodos que gravam fichas, regista na auditoria o antes e o depois
/// (services/auditoria.rs); no fim, sela as linhas novas da auditoria (src/auditoria.rs).
pub async fn dispatch(st: &AppState, metodo: &str, params: Value) -> AppResult<Value> {
    let r = match auditoria::alvo(metodo, &params) {
        None => Box::pin(executar(st, metodo, params)).await,
        Some(alvo) => {
            let antes = auditoria::antes(st, &alvo, &params).await;
            let r = Box::pin(executar(st, metodo, params.clone())).await;
            if let Ok(v) = &r {
                if let Err(e) = auditoria::depois(st, &alvo, &params, antes, v).await {
                    eprintln!("Auditoria: {metodo} não ficou registado ({e}).");
                }
            }
            r
        }
    };
    if let Err(e) = crate::auditoria::selar_novas(&st.pool).await {
        eprintln!("Auditoria: falhou o selo das linhas novas ({e}).");
    }
    r
}

async fn executar(st: &AppState, metodo: &str, params: Value) -> AppResult<Value> {
    // Senha provisória por mudar ou sessão bloqueada: só se pode mudar a senha, retomar ou sair.
    let livre = matches!(
        metodo,
        "setup.estado" | "setup.inicializar" | "setup.restaurar" | "auth.login" | "auth.loginPin" | "auth.logout" | "auth.sessao" | "auth.alterarSenha" | "auth.desbloquear" | "seguranca.politica"
            | "auth.recuperarVerificar" | "auth.recuperarConcluir"
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
    // Módulos com despachante próprio, num futuro em caixa: o `match` abaixo já é muito grande e cada caso
    // novo aumenta a pilha que ele usa (nas compilações de desenvolvimento chegava ao limite da pilha).
    if let Some(resto) = metodo.strip_prefix("avencas.") {
        return Box::pin(avencas::despachar(st, resto, params)).await;
    }
    if let Some(resto) = metodo.strip_prefix("modo.") {
        return Box::pin(modos::despachar(st, resto, params)).await;
    }
    if let Some(resto) = metodo.strip_prefix("escola.") {
        return Box::pin(escola::despachar(st, resto, params)).await;
    }
    if let Some(resto) = metodo.strip_prefix("tv.") {
        return Box::pin(tv::despachar(st, resto, params)).await;
    }
    if let Some(resto) = metodo.strip_prefix("fotos.") {
        return Box::pin(async move {
            match resto {
                "guardar" => fotos::guardar(st, p(params)?).await,
                "apagar" => fotos::apagar(st, p(params)?).await,
                "obter" => fotos::obter(st, p(params)?).await,
                "miniaturas" => fotos::miniaturas(st, p(params)?).await,
                _ => Err(AppError::msg(format!("Método desconhecido: fotos.{resto}"))),
            }
        })
        .await;
    }
    match metodo {
        // ---- públicos ----
        "setup.estado" => setup::estado(st).await,
        "setup.inicializar" => setup::inicializar(st, p(params)?).await,
        "setup.restaurar" => sistema::restaurar_inicial(st, p(params)?).await,
        "auth.login" => auth::login(st, p(params)?).await,
        "auth.logout" => auth::logout(st).await,
        "auth.loginPin" => auth::login_pin(st, p(params)?).await,
        "auth.recuperarVerificar" => auth::recuperar_verificar(st, p(params)?).await,
        "auth.perfil" => auth::perfil(st).await,
        "auth.guardarPerfil" => auth::guardar_perfil(st, p(params)?).await,
        "aparencia.guardar" => aparencia::guardar(st, p(params)?).await,
        "auth.recuperarConcluir" => auth::recuperar_concluir(st, p(params)?).await,
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
        "auditoria.listar" => auditoria::listar(st, p(params)?).await,
        "auditoria.opcoes" => auditoria::opcoes(st).await,
        "auditoria.historico" => auditoria::historico(st, p(params)?).await,
        "auditoria.verificar" => auditoria::verificar(st).await,
        "auth.alterarSenha" => auth::alterar_senha(st, p(params)?).await,
        "auth.sessoes" => auth::sessoes(st, if params.is_null() { auth::SessoesIn::default() } else { p(params)? }).await,
        "auth.terminarSessao" => auth::terminar_sessao(st, p(params)?).await,
        "auth.terminarOutras" => auth::terminar_outras(st).await,
        "auth.autorizar" => auth::autorizar(st, p(params)?).await,
        "empresa.obter" => setup::empresa_obter(st).await,
        "empresa.guardar" => setup::empresa_guardar(st, p(params)?).await,
        "agt.info" => setup::agt_info(st).await,
        "agt.guardarCertificado" => setup::agt_guardar(st, p(params)?).await,
        "agt.importarChave" => setup::agt_importar_chave(st, p(params)?).await,
        "agt.verificarIntegridade" => integridade::verificar(st).await,

        // ---- importação do KIGEST (SQL Server) ----
        "importar.analisar" => importar::analisar(st, p(params)?).await,
        "importar.executar" => importar::executar(st, p(params)?).await,
        "importar.analisarFicheiro" => importar::analisar_ficheiro(st, p(params)?).await,
        "importar.executarFicheiro" => importar::executar_ficheiro(st, p(params)?).await,
        "importar.ficheiros" => importar::ficheiros(st).await,
        // o KIGEST neste computador: Conexao.ini (sem a senha), instâncias do SQL Server e ficheiros .kigest
        "importar.descobrir" => importar::descobrir(st).await,
        // importação universal por SAF-T (AO): clientes, fornecedores e artigos de qualquer sistema certificado
        "importar.analisarSaft" => importar_saft::analisar(st, p(params)?).await,
        "importar.executarSaft" => importar_saft::executar(st, p(params)?).await,
        "importar.analisarFolha" => importar_folha::analisar(st, p(params)?).await,
        "importar.executarFolha" => importar_folha::executar(st, p(params)?).await,

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

        // ---- API de integração (chaves de acesso para outros sistemas) ----
        "api.estado" => crate::api::estado(st).await,
        "api.guardar" => crate::api::guardar(st, p(params)?).await,
        "api.chaveCriar" => crate::api::chave_criar(st, p(params)?).await,
        "api.chaveGuardar" => crate::api::chave_guardar(st, p(params)?).await,
        "api.chaveRenovar" => crate::api::chave_renovar(st, p(params)?).await,
        "api.chaveRevogar" => crate::api::chave_revogar(st, p(params)?).await,
        "api.pedidos" => crate::api::pedidos(st, p(params)?).await,
        "api.webhooks" => crate::webhooks::listar(st).await,
        "api.webhookGuardar" => crate::webhooks::guardar(st, p(params)?).await,
        "api.webhookApagar" => crate::webhooks::apagar(st, p(params)?).await,
        "api.webhookSegredo" => crate::webhooks::renovar_segredo(st, p(params)?).await,
        "api.webhookTestar" => crate::webhooks::testar(st, p(params)?).await,
        "api.entregaReenviar" => crate::webhooks::reenviar(st, p(params)?).await,

        // ---- verificação em dois passos (TOTP) ----
        "auth.doisPassosEstado" => dois_passos::estado(st).await,
        "auth.doisPassosNovo" => dois_passos::novo(st).await,
        "auth.doisPassosLigar" => dois_passos::ligar(st, p(params)?).await,
        "auth.doisPassosDesligar" => dois_passos::desligar(st, p(params)?).await,
        "utilizadores.doisPassosRepor" => dois_passos::repor(st, p(params)?).await,

        // ---- acesso pela Internet (subdomínio do KanGest por túnel da Cloudflare) ----
        "web.estado" => acesso_web::estado(st, p(params)?).await,
        "web.activar" => acesso_web::activar(st, p(params)?).await,
        "web.activarProprio" => acesso_web::activar_proprio(st, p(params)?).await,
        "web.desactivar" => acesso_web::desactivar(st).await,

        // ---- postos em rede (servidor / cliente) ----
        "rede.estado" => crate::rede::estado(st).await,
        "rede.guardar" => crate::rede::guardar(st, p(params)?).await,
        "rede.testar" => crate::rede::testar(st).await,
        // arranque automático deste computador (método local: cada computador tem o seu; src/nativo.rs)
        "rede.arranque" => crate::nativo::arranque(st).await,
        "rede.arranqueGuardar" => crate::nativo::arranque_guardar(st, p(params)?).await,

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
        "artigos.eliminar" => catalogo::artigos_eliminar(st, p(params)?).await,
        "artigos.lote" => catalogo::artigos_lote(st, p(params)?).await,
        "artigos.codigoUsado" => catalogo::codigo_usado(st, p(params)?).await,
        "artigos.associarCodigo" => catalogo::associar_codigo(st, p(params)?).await,
        "stock.listar" => catalogo::stock_listar(st, p(params)?).await,
        "stock.movimentos" => catalogo::stock_movimentos(st, p(params)?).await,
        "stock.recalcular" => catalogo::stock_recalcular(st, p(params)?).await,
        "artigos.variantes" => ramos::variantes(st, p(params)?).await,
        "ramo.obter" => ramos::obter(st).await,
        "ramo.definir" => ramos::definir(st, p(params)?).await,
        "artigos.componentes" => catalogo::artigos_componentes(st, p(params)?).await,
        "lotes.listar" => catalogo::lotes_listar(st, p(params)?).await,
        "moedas.listar" => catalogo::moedas(st).await,
        "moedas.guardar" => catalogo::moeda_guardar(st, p(params)?).await,

        // ---- entidades ----
        "entidades.listar" => entidades::listar(st, p(params)?).await,
        "entidades.obter" => entidades::obter(st, p(params)?).await,
        "entidades.guardar" => entidades::guardar(st, p(params)?).await,
        "entidades.eliminar" => entidades::eliminar(st, p(params)?).await,
        "entidades.lote" => entidades::lote(st, p(params)?).await,
        "entidades.nifUsado" => entidades::nif_usado(st, p(params)?).await,
        "entidades.pendentes" => entidades::pendentes(st, p(params)?).await,
        "entidades.enderecos" => entidades::enderecos(st, p(params)?).await,
        "entidades.enderecoGuardar" => entidades::endereco_guardar(st, p(params)?).await,

        // ---- documentos ----
        "documentos.emitir" => documentos::emitir(st, p(params)?).await,
        "documentos.listar" => documentos::listar(st, p(params)?).await,
        "documentos.obter" => documentos::obter(st, p(params)?).await,
        "documentos.anular" => documentos::anular(st, p(params)?).await,
        "documentos.creditavel" => documentos::creditavel(st, p(params)?).await,
        "documentos.auxiliares" => documentos::auxiliares(st).await,

        // ---- promoções por regras e cupões ----
        "promocoes.calcular" => promocoes::calcular(st, p(params)?).await,
        "promocoes.listar" => promocoes::listar(st).await,
        "promocoes.regraGuardar" => promocoes::regra_guardar(st, p(params)?).await,
        "promocoes.cupaoGuardar" => promocoes::cupao_guardar(st, p(params)?).await,
        "cupoes.validar" => promocoes::validar(st, p(params)?).await,

        // ---- sugestão de encomendas e lançamentos para a contabilidade ----
        "compras.sugestao" => gestao::sugestao(st, p(params)?).await,
        "contabilidade.contas" => gestao::contas_obter(st).await,
        "contabilidade.contasGuardar" => gestao::contas_guardar(st, p(params)?).await,
        "contabilidade.lancamentos" => gestao::lancamentos(st, p(params)?).await,

        // ---- fidelização de clientes (pontos) ----
        "fidelizacao.config" => fidelizacao::config_obter(st).await,
        "fidelizacao.configGuardar" => fidelizacao::config_guardar(st, p(params)?).await,
        "fidelizacao.saldo" => fidelizacao::saldo(st, p(params)?).await,
        "fidelizacao.clientes" => fidelizacao::clientes(st, p(params)?).await,
        "fidelizacao.movimentos" => fidelizacao::movimentos(st, p(params)?).await,
        "fidelizacao.ajustar" => fidelizacao::ajustar(st, p(params)?).await,

        // ---- recibo digital: email e WhatsApp ----
        "email.config" => partilha::config_obter(st).await,
        "email.configGuardar" => partilha::config_guardar(st, p(params)?).await,
        "email.testar" => partilha::testar(st, p(params)?).await,
        "documentos.enviarEmail" => partilha::enviar_documento(st, p(params)?).await,
        "documentos.mensagem" => partilha::mensagem(st, p(params)?).await,
        // local: a ligação abre-se no posto onde está o operador
        "partilha.abrir" => partilha::abrir(p(params)?).await,

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
        "caixa.movimentoObter" => caixa::movimento_obter(st, p(params)?).await,
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
        "internet.estado" => ligacao::estado(st).await,
        "internet.guardar" => ligacao::guardar(st, p(params)?).await,
        "internet.diagnostico" => ligacao::diagnostico(st).await,
        "comissoes.taxas" => comissoes::taxas(st).await,
        "comissoes.taxaGuardar" => comissoes::taxa_guardar(st, p(params)?).await,
        "comissoes.relatorio" => comissoes::relatorio(st, p(params)?).await,
        "comissoes.lancar" => comissoes::lancar(st, p(params)?).await,
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
        "oficina.ordemEstado" => oficina::ordem_estado(st, p(params)?).await,
        // livros Excel gerados no ecrã (num posto cliente gravam-se no próprio posto: prefixo «sistema.exportar»)
        "sistema.exportarBinario" => ficheiros::exportar_binario(st, p(params)?).await,
        // SAF-T de inventários (AGT, Circular n.º 05/GACA/GJ/DSF/AGT/2026): existências a 31/12
        "saft.inventario" => inventario::inventario(st, p(params)?).await,
        // quadro de propostas e encomendas (kanban)
        "documentos.quadro" => quadro::quadro(st, p(params)?).await,
        "oficina.associar" => oficina::associar(st, p(params)?).await,

        // ---- relatórios / sistema ----
        "dashboard" => relatorios::dashboard(st).await,
        // KanGest inteligente (calculado aqui, nada sai para fora): resumo de ontem, previsões e alertas
        "inteligencia.painel" => inteligencia::painel(st).await,
        // assistente de perguntas (Ctrl+K): «quanto vendi ontem?», «quem me deve mais?»
        "inteligencia.perguntar" => assistente::perguntar(st, p(params)?).await,
        // resumo diário ao dono por email (opcional), pelo servidor de email da empresa
        "inteligencia.resumoConfig" => resumo_email::config_obter(st).await,
        "inteligencia.resumoGuardar" => resumo_email::config_guardar(st, p(params)?).await,
        "inteligencia.resumoEnviar" => resumo_email::enviar_agora(st).await,
        "relatorios.vendas" => relatorios::vendas(st, p(params)?).await,
        "dados.fontes" => dados::fontes_listar(st).await,
        "dados.consultar" => dados::consultar(st, p(params)?).await,
        "dados.modelos" => dados::modelos_listar(st).await,
        "dados.modeloGuardar" => dados::modelo_guardar(st, p(params)?).await,
        "dados.modeloEliminar" => dados::modelo_eliminar(st, p(params)?).await,
        "relatorios.sintese" => relatorios::sintese(st, p(params)?).await,
        "relatorios.top" => relatorios::top(st, p(params)?).await,
        "relatorios.irtC" => relatorios::irt_c_obter(st).await,
        "relatorios.irtCGuardar" => relatorios::irt_c_guardar(st, p(params)?).await,
        "relatorios.stock" => relatorios::stock(st, p(params)?).await,
        "relatorios.rastreio" => relatorios::rastreio(st, p(params)?).await,
        "relatorios.fornecedores" => relatorios::fornecedores(st, p(params)?).await,
        "saft.exportar" => saft::exportar(st, p(params)?).await,
        "saft.exportarCompras" => saft::exportar_compras(st, p(params)?).await,
        "saft.exportarEstado" => saft::exportar_estado(st, p(params)?).await,
        "sistema.backup" => sistema::backup(st, p(params)?).await,
        "sistema.copias" => sistema::copias(st).await,
        "sistema.copiasGuardar" => sistema::copias_guardar(st, p(params)?).await,
        "sistema.copiaAgora" => sistema::copia_agora(st).await,
        "sistema.restaurar" => sistema::restaurar(st, p(params)?).await,
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
