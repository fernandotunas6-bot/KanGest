//! Licença do KanGest, verificada no próprio computador (sem internet).
//!
//! Uma licença é «KG1.<dados>.<assinatura>»: os dados (JSON em base64url) dizem o NIF da empresa, o plano, os postos
//! e o período; a assinatura Ed25519 é feita pelo produtor com a chave privada (scripts/licencas/kangest-licencas.mjs)
//! e confirmada aqui com a chave pública. Sem licença, o KanGest funciona DIAS_EXPERIENCIA dias desde a instalação.
//! Quando a licença (ou a experiência) termina, passa a modo de consulta: lê-se, imprime-se e exporta-se o SAF-T,
//! mas não se emitem documentos novos.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Duration, NaiveDate, NaiveDateTime};
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::sync::OnceLock;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Chaves públicas do produtor (base64url de 32 bytes). Uma lista para poder trocar de chave sem invalidar as antigas.
const CHAVES_PUBLICAS: &[&str] = &["2_760imzJ6c3wizg0oU01eSyp9vavAiFzZNIX-iBysI"];
/// Período de experiência sem licença. Não vem da configuração (que o cliente pode alterar).
pub const DIAS_EXPERIENCIA: i64 = 30;
/// Quanto o relógio pode estar atrás da última data vista antes de se suspeitar de batota.
const HORAS_RELOGIO: i64 = 36;
/// Avisar quando faltam estes dias.
const DIAS_AVISO: i64 = 15;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Licenca {
    pub v: u8,
    pub id: String,
    pub nif: String,
    pub empresa: String,
    pub plano: String,
    /// Postos permitidos (0 = sem limite).
    pub postos: i64,
    #[serde(default)]
    pub modulos: Vec<String>,
    pub inicio: String,
    pub fim: String,
    #[serde(default = "tolerancia_padrao")]
    pub tolerancia: i64,
    #[serde(default)]
    pub maquina: Option<String>,
    #[serde(default)]
    pub emitida: String,
}

fn tolerancia_padrao() -> i64 {
    15
}

fn chaves() -> Vec<Vec<u8>> {
    #[allow(unused_mut)]
    let mut v: Vec<Vec<u8>> = CHAVES_PUBLICAS.iter().filter_map(|k| URL_SAFE_NO_PAD.decode(k).ok()).collect();
    #[cfg(test)]
    v.push(teste::chave_publica());
    v
}

/// Lê e confirma uma licença. Erros em português, prontos a mostrar.
pub fn ler(texto: &str) -> Result<Licenca, String> {
    let chave: String = texto.chars().filter(|c| !c.is_whitespace()).collect();
    let mut partes = chave.split('.');
    let (pref, corpo, assinatura) = (partes.next(), partes.next(), partes.next());
    let (Some("KG1"), Some(corpo), Some(assinatura), None) = (pref, corpo, assinatura, partes.next()) else {
        return Err("Isto não é uma licença do KanGest. Copie o texto completo, que começa por «KG1.».".into());
    };
    let assinatura = URL_SAFE_NO_PAD.decode(assinatura).map_err(|_| "A licença está incompleta ou foi alterada.".to_string())?;
    let valida = chaves().iter().any(|k| UnparsedPublicKey::new(&ED25519, k).verify(corpo.as_bytes(), &assinatura).is_ok());
    if !valida {
        return Err("A licença não é válida: foi alterada ou não foi emitida pelo produtor do KanGest.".into());
    }
    let dados = URL_SAFE_NO_PAD.decode(corpo).map_err(|_| "A licença está incompleta.".to_string())?;
    serde_json::from_slice(&dados).map_err(|_| "A licença tem um formato desconhecido. Actualize o KanGest.".to_string())
}

// ------------------------------------------------------------------ identificação do computador

/// Identificador deste computador (derivado do do sistema operativo), ex. «3F9A-12BC-77D0».
pub fn id_maquina() -> String {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| {
        let bruto = id_sistema().unwrap_or_else(|| "sem-identificador".into());
        let h = Sha256::digest(format!("kangest:{}", bruto.trim()).as_bytes());
        let hex: String = h.iter().take(6).map(|b| format!("{b:02X}")).collect();
        format!("{}-{}-{}", &hex[0..4], &hex[4..8], &hex[8..12])
    })
    .clone()
}

#[cfg(target_os = "macos")]
fn id_sistema() -> Option<String> {
    let s = std::process::Command::new("ioreg").args(["-rd1", "-c", "IOPlatformExpertDevice"]).output().ok()?;
    let txt = String::from_utf8_lossy(&s.stdout);
    let linha = txt.lines().find(|l| l.contains("IOPlatformUUID"))?;
    linha.split('"').nth(3).map(str::to_string)
}

#[cfg(target_os = "windows")]
fn id_sistema() -> Option<String> {
    let mut c = std::process::Command::new("reg");
    c.args(["query", r"HKLM\SOFTWARE\Microsoft\Cryptography", "/v", "MachineGuid"]);
    super::impressao::sem_consola(&mut c);
    let s = c.output().ok()?;
    String::from_utf8_lossy(&s.stdout).lines().find(|l| l.contains("MachineGuid"))?.split_whitespace().last().map(str::to_string)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn id_sistema() -> Option<String> {
    std::fs::read_to_string("/etc/machine-id").ok()
}

// ------------------------------------------------------------------ estado

async fn cfg_ler(p: &SqlitePool, chave: &str) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(chave).fetch_optional(p).await?)
}

async fn cfg_gravar(p: &SqlitePool, chave: &str, valor: &str) -> AppResult<()> {
    sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor").bind(chave).bind(valor).execute(p).await?;
    Ok(())
}

fn so_algarismos(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase()
}

#[derive(Debug, Clone, Serialize)]
pub struct Estado {
    /// EXPERIENCIA · ACTIVA · TOLERANCIA · EXPIRADA · RELOGIO · INVALIDA
    pub modo: &'static str,
    pub pode_emitir: bool,
    /// Dias que faltam até deixar de poder emitir (0 quando já não pode).
    pub dias: i64,
    pub aviso: bool,
    pub mensagem: String,
    pub fim: Option<String>,
    pub plano: String,
    pub postos: i64,
    pub licenca_id: Option<String>,
    pub empresa: Option<String>,
    pub nif: Option<String>,
    pub maquina: String,
    /// Texto que o cliente envia ao produtor para pedir a licença (NIF + computador).
    pub pedido: String,
}

/// Data de instalação (o início do período de experiência). Nas bases antigas, conta a partir de hoje.
async fn instalada_em(p: &SqlitePool) -> AppResult<NaiveDate> {
    if let Some(d) = cfg_ler(p, "licenca.instalada_em").await?.and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()) {
        return Ok(d);
    }
    let hoje = db::hoje();
    cfg_gravar(p, "licenca.instalada_em", &hoje).await?;
    Ok(NaiveDate::parse_from_str(&hoje, "%Y-%m-%d").expect("data de hoje"))
}

/// Marca o início da experiência ao criar a empresa.
pub async fn registar_instalacao(p: &SqlitePool) -> AppResult<()> {
    cfg_gravar(p, "licenca.instalada_em", &db::hoje()).await
}

/// O relógio só pode andar para a frente: guarda a data mais recente vista e detecta um atraso grande.
async fn relogio_atrasado(p: &SqlitePool) -> AppResult<Option<String>> {
    let agora_txt = db::agora();
    let agora = NaiveDateTime::parse_from_str(&agora_txt, "%Y-%m-%dT%H:%M:%S").expect("data de agora");
    let visto = cfg_ler(p, "licenca.ultimo_visto").await?.and_then(|v| NaiveDateTime::parse_from_str(&v, "%Y-%m-%dT%H:%M:%S").ok());
    match visto {
        Some(v) if agora < v - Duration::hours(HORAS_RELOGIO) => Ok(Some(v.format("%d/%m/%Y %H:%M").to_string())),
        Some(v) if agora <= v => Ok(None),
        _ => {
            cfg_gravar(p, "licenca.ultimo_visto", &agora_txt).await?;
            Ok(None)
        }
    }
}

fn data(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

fn dmy(d: NaiveDate) -> String {
    d.format("%d/%m/%Y").to_string()
}

pub async fn calcular(st: &AppState) -> AppResult<Estado> {
    let p = &st.pool;
    let hoje = data(&db::hoje()).expect("hoje");
    let nif_empresa: Option<String> = sqlx::query_scalar("SELECT nif FROM empresa WHERE id = 1").fetch_optional(p).await?;
    let maquina = id_maquina();
    let pedido = format!("KG-{}-{}", nif_empresa.as_deref().map(so_algarismos).unwrap_or_default(), maquina.replace('-', ""));
    let mut e = Estado {
        modo: "EXPERIENCIA",
        pode_emitir: true,
        dias: 0,
        aviso: false,
        mensagem: String::new(),
        fim: None,
        plano: "EXPERIENCIA".into(),
        postos: 0,
        licenca_id: None,
        empresa: None,
        nif: nif_empresa.clone(),
        maquina,
        pedido,
    };

    if let Some(visto) = relogio_atrasado(p).await? {
        e.modo = "RELOGIO";
        e.pode_emitir = false;
        e.mensagem = format!("A data deste computador está atrasada em relação à última utilização ({visto}). Acerte a data e a hora do sistema para voltar a facturar.");
        return Ok(e);
    }

    // licença activada
    let mut problema: Option<String> = None;
    if let Some(texto) = cfg_ler(p, "licenca.chave").await?.filter(|t| !t.trim().is_empty()) {
        match ler(&texto) {
            Err(erro) => problema = Some(erro),
            Ok(lic) => {
                let nif_ok = nif_empresa.as_deref().map(so_algarismos) == Some(so_algarismos(&lic.nif));
                let maquina_ok = lic.maquina.as_deref().map_or(true, |m| so_algarismos(m) == so_algarismos(&e.maquina));
                if !nif_ok {
                    problema = Some(format!("A licença activada é da empresa com o NIF {}, e esta empresa tem o NIF {}.", lic.nif, nif_empresa.clone().unwrap_or_default()));
                } else if !maquina_ok {
                    problema = Some("A licença activada é de outro computador. Peça uma licença para este (código do pedido abaixo).".into());
                } else if let (Some(inicio), Some(fim)) = (data(&lic.inicio), data(&lic.fim)) {
                    e.plano = lic.plano.clone();
                    e.postos = lic.postos;
                    e.licenca_id = Some(lic.id.clone());
                    e.empresa = Some(lic.empresa.clone());
                    e.fim = Some(lic.fim.clone());
                    let limite = fim + Duration::days(lic.tolerancia.max(0));
                    if hoje < inicio {
                        e.modo = "INVALIDA";
                        e.pode_emitir = false;
                        e.mensagem = format!("A licença só começa a {}.", dmy(inicio));
                    } else if hoje <= fim {
                        e.modo = "ACTIVA";
                        e.dias = (fim - hoje).num_days() + 1;
                        e.aviso = e.dias <= DIAS_AVISO;
                        e.mensagem = format!("Licença {} válida até {}.", lic.plano, dmy(fim));
                    } else if hoje <= limite {
                        e.modo = "TOLERANCIA";
                        e.dias = (limite - hoje).num_days() + 1;
                        e.aviso = true;
                        e.mensagem = format!("A assinatura terminou a {}. Pode continuar a facturar até {}: renove para não parar.", dmy(fim), dmy(limite));
                    } else {
                        e.modo = "EXPIRADA";
                        e.pode_emitir = false;
                        e.mensagem = format!("A assinatura terminou a {}. Pode consultar e imprimir, mas não emitir documentos novos. Renove em Sistema → Licença e Assinatura.", dmy(fim));
                    }
                    return Ok(e);
                }
            }
        }
    }

    // período de experiência
    let inicio = instalada_em(p).await?;
    let fim = inicio + Duration::days(DIAS_EXPERIENCIA - 1);
    e.fim = Some(fim.format("%Y-%m-%d").to_string());
    if hoje <= fim {
        e.dias = (fim - hoje).num_days() + 1;
        e.aviso = e.dias <= DIAS_AVISO;
        e.mensagem = format!("Período de experiência: faltam {} {} (até {}).", e.dias, if e.dias == 1 { "dia" } else { "dias" }, dmy(fim));
    } else {
        e.modo = "EXPIRADA";
        e.pode_emitir = false;
        e.mensagem = format!("O período de experiência terminou a {}. Pode consultar e imprimir, mas não emitir documentos novos. Active uma licença em Sistema → Licença e Assinatura.", dmy(fim));
    }
    if let Some(p) = problema {
        e.mensagem = format!("{p} {}", e.mensagem);
        if e.modo == "EXPERIENCIA" {
            e.modo = "INVALIDA";
        }
    }
    Ok(e)
}

/// Chamado antes de emitir qualquer documento.
pub async fn exigir_emissao(st: &AppState) -> AppResult<()> {
    let e = calcular(st).await?;
    if e.pode_emitir {
        Ok(())
    } else {
        Err(AppError::msg(e.mensagem))
    }
}

/// Postos permitidos pela licença (None = sem limite).
pub async fn limite_postos(st: &AppState) -> AppResult<Option<i64>> {
    let e = calcular(st).await?;
    Ok((matches!(e.modo, "ACTIVA" | "TOLERANCIA") && e.postos > 0).then_some(e.postos))
}

// ------------------------------------------------------------------ RPC

pub async fn estado(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    Ok(serde_json::to_value(calcular(st).await?)?)
}

#[derive(Deserialize)]
pub struct ActivarIn {
    chave: String,
}

pub async fn activar(st: &AppState, i: ActivarIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.configuracoes")?;
    let lic = ler(&i.chave).map_err(AppError::msg)?;
    let nif: String = sqlx::query_scalar("SELECT nif FROM empresa WHERE id = 1").fetch_one(&st.pool).await?;
    if so_algarismos(&nif) != so_algarismos(&lic.nif) {
        return Err(AppError::msg(format!("Esta licença é da empresa com o NIF {} ({}). A empresa configurada tem o NIF {nif}.", lic.nif, lic.empresa)));
    }
    if let Some(m) = &lic.maquina {
        if so_algarismos(m) != so_algarismos(&id_maquina()) {
            return Err(AppError::msg(format!("Esta licença é do computador {m}; este é o {}.", id_maquina())));
        }
    }
    let fim = data(&lic.fim).ok_or_else(|| AppError::msg("A licença tem uma data inválida."))?;
    let hoje = data(&db::hoje()).expect("hoje");
    if hoje > fim + Duration::days(lic.tolerancia.max(0)) {
        return Err(AppError::msg(format!("Esta licença terminou a {}. Peça a renovação.", dmy(fim))));
    }
    let texto: String = i.chave.chars().filter(|c| !c.is_whitespace()).collect();
    cfg_gravar(&st.pool, "licenca.chave", &texto).await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'LICENCA', ?)")
        .bind(s.id)
        .bind(format!("Licença {} activada: {} até {}, {} postos", lic.id, lic.plano, lic.fim, lic.postos))
        .execute(&st.pool)
        .await?;
    Ok(json!({ "licenca": lic, "estado": calcular(st).await? }))
}

#[derive(Deserialize)]
pub struct AbrirIn {
    url: String,
}

/// Abre no navegador a página onde se pede ou renova a licença. Só aceita o site do KanGest.
pub async fn abrir_site(i: AbrirIn) -> AppResult<Value> {
    if !i.url.starts_with("https://kangest.pages.dev/") || i.url.chars().any(|c| c.is_whitespace() || c == '"') {
        return Err(AppError::msg("Endereço não permitido."));
    }
    #[cfg(target_os = "windows")]
    let r = {
        let mut c = std::process::Command::new("rundll32");
        super::impressao::sem_consola(&mut c);
        c.args(["url.dll,FileProtocolHandler", &i.url]).spawn()
    };
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(&i.url).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(&i.url).spawn();
    r.map_err(|e| AppError::msg(format!("Não foi possível abrir o navegador: {e}")))?;
    Ok(Value::Null)
}

#[cfg(test)]
mod testes_produtor {
    /// Licença de exemplo (empresa fictícia) emitida com a chave verdadeira pelo scripts/licencas/kangest-licencas.mjs:
    /// prova que o que o Node assina, o KanGest aceita.
    #[test]
    fn licenca_do_produtor_e_aceite() {
        let l = super::ler("KG1.eyJ2IjoxLCJpZCI6IkxJQy1DQTIxQ0NDNDQ2IiwibmlmIjoiNTAwMDAwMDAwMCIsImVtcHJlc2EiOiJFbXByZXNhIEV4ZW1wbG8sIExkYSIsInBsYW5vIjoiRVNTRU5DSUFMIiwicG9zdG9zIjoxLCJtb2R1bG9zIjpbXSwiaW5pY2lvIjoiMjAyNi0wMS0wMSIsImZpbSI6IjIwMjYtMDEtMzEiLCJ0b2xlcmFuY2lhIjoxNSwiZW1pdGlkYSI6IjIwMjYtMTEtMTIifQ.lkYNQv_AXYmvKcTKShZP32NDujI4ETHDk1HJ-tcn80qkaH0lHSUB5K3RV_qjg2AVI242KefrIkQNZ0TX2t5wCw").expect("licença do produtor");
        assert_eq!((l.nif.as_str(), l.plano.as_str(), l.postos, l.fim.as_str()), ("5000000000", "ESSENCIAL", 1, "2026-01-31"));
        let mut alterada = "KG1.eyJ2IjoxLCJpZCI6IkxJQy1DQTIxQ0NDNDQ2IiwibmlmIjoiNTAwMDAwMDAwMCIsImVtcHJlc2EiOiJFbXByZXNhIEV4ZW1wbG8sIExkYSIsInBsYW5vIjoiRVNTRU5DSUFMIiwicG9zdG9zIjoxLCJtb2R1bG9zIjpbXSwiaW5pY2lvIjoiMjAyNi0wMS0wMSIsImZpbSI6IjIwMjYtMDEtMzEiLCJ0b2xlcmFuY2lhIjoxNSwiZW1pdGlkYSI6IjIwMjYtMTEtMTIifQ.lkYNQv_AXYmvKcTKShZP32NDujI4ETHDk1HJ-tcn80qkaH0lHSUB5K3RV_qjg2AVI242KefrIkQNZ0TX2t5wCw".to_string();
        alterada.insert(30, 'x');
        assert!(super::ler(&alterada).is_err());
    }
}

#[cfg(test)]
pub mod teste {
    //! Chave de teste: só os testes a aceitam (a do produtor nunca está no código).
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn par() -> Ed25519KeyPair {
        Ed25519KeyPair::from_seed_unchecked(&[7u8; 32]).expect("par de teste")
    }

    pub fn chave_publica() -> Vec<u8> {
        par().public_key().as_ref().to_vec()
    }

    /// Assina uma licença de teste (os mesmos dados que o kangest-licencas.mjs).
    pub fn emitir(dados: &serde_json::Value) -> String {
        let corpo = URL_SAFE_NO_PAD.encode(serde_json::to_vec(dados).unwrap());
        let assinatura = URL_SAFE_NO_PAD.encode(par().sign(corpo.as_bytes()).as_ref());
        format!("KG1.{corpo}.{assinatura}")
    }
}
