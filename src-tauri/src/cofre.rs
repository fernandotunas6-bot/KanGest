//! Segredos guardados na base de dados, cifrados (AES-256-GCM).
//!
//! Cifra a chave privada de assinatura dos documentos (AGT), a senha e as chaves da Facturação Electrónica e os segredos
//! dos avisos automáticos (webhooks). A chave da cifra sai de uma chave do programa (que não está inteira no executável)
//! e do sal da própria base (`cofre.sal`): uma cópia da base continua a abrir noutro computador com o KanGest (cópia de
//! segurança, computador novo), mas quem abre a base com um editor de SQLite, ou lê uma cópia, vê `KGC1.…` em vez da
//! chave ou da senha. Cada valor vai ligado ao sítio onde está guardado: copiado para outro campo, não abre.
//!
//! Os valores antigos, em texto, continuam a ler-se e são cifrados no arranque ([`migrar`]). Limites em
//! docs/SEGURANCA.md: quem estudar o programa encontra a chave do programa.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

pub const PREFIXO: &str = "KGC1.";
/// Segredos guardados na tabela `config`.
pub const CHAVES_CONFIG: [&str; 4] = ["agt.chave_privada", "fe.senha", "fe.chave_software", "fe.chave_contribuinte"];
/// Contexto dos segredos dos avisos automáticos (`api_webhooks.segredo`).
pub const WEBHOOK: &str = "api_webhooks.segredo";

fn chave_programa() -> [u8; 32] {
    // não está escrita por inteiro no executável: duas metades que só se juntam ao correr
    const A: [u8; 32] = [81, 23, 123, 250, 144, 70, 180, 188, 43, 218, 17, 159, 194, 151, 158, 249, 121, 57, 44, 224, 238, 186, 75, 214, 75, 87, 19, 207, 49, 225, 128, 61];
    const B: [u8; 32] = [190, 182, 231, 28, 55, 238, 152, 174, 200, 155, 216, 140, 195, 185, 143, 197, 20, 243, 23, 249, 133, 79, 195, 221, 79, 198, 152, 215, 116, 126, 221, 253];
    let mut k = [0u8; 32];
    for (i, b) in k.iter_mut().enumerate() {
        *b = std::hint::black_box(B[i]) ^ A[31 - i];
    }
    k
}

/// Chave da cifra desta base. O sal é criado pela migração 0022; se alguém o apagar, cria-se outro (os segredos
/// cifrados com o anterior deixam de abrir, como se tivessem sido apagados).
async fn chave(p: &SqlitePool) -> AppResult<LessSafeKey> {
    let ler = || sqlx::query_scalar::<_, String>("SELECT valor FROM config WHERE chave = 'cofre.sal'").fetch_optional(p);
    let sal = match ler().await? {
        Some(s) if !s.trim().is_empty() => s,
        _ => {
            let novo: [u8; 16] = aleatorio()?;
            let hex: String = novo.iter().map(|b| format!("{b:02x}")).collect();
            sqlx::query("INSERT INTO config (chave, valor) VALUES ('cofre.sal', ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor WHERE trim(config.valor) = ''")
                .bind(hex)
                .execute(p)
                .await?;
            ler().await?.unwrap_or_default()
        }
    };
    let mut h = Sha256::new();
    h.update(b"KanGest-cofre-v1");
    h.update(chave_programa());
    h.update(sal.trim().as_bytes());
    let k: [u8; 32] = h.finalize().into();
    Ok(LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &k).map_err(|_| AppError::msg("Chave do cofre inválida."))?))
}

fn aleatorio<const N: usize>() -> AppResult<[u8; N]> {
    let mut b = [0u8; N];
    SystemRandom::new().fill(&mut b).map_err(|_| AppError::msg("Sem números aleatórios do sistema."))?;
    Ok(b)
}

fn ilegivel(contexto: &str) -> AppError {
    AppError::msg(format!(
        "O segredo guardado em «{contexto}» não abre: a base foi alterada fora do KanGest ou o valor foi copiado de outra base. Volte a introduzi-lo."
    ))
}

/// Cifra um segredo para guardar em `contexto` (ex.: `config:fe.senha`). Vazio continua vazio.
pub async fn cifrar(p: &SqlitePool, contexto: &str, claro: &str) -> AppResult<String> {
    if claro.is_empty() {
        return Ok(String::new());
    }
    let n: [u8; 12] = aleatorio()?;
    let mut buf = claro.as_bytes().to_vec();
    chave(p)
        .await?
        .seal_in_place_append_tag(Nonce::assume_unique_for_key(n), Aad::from(contexto.as_bytes()), &mut buf)
        .map_err(|_| AppError::msg("Não foi possível cifrar o segredo."))?;
    Ok(format!("{PREFIXO}{}", URL_SAFE_NO_PAD.encode([&n[..], &buf].concat())))
}

/// Abre um segredo guardado em `contexto`. Os valores antigos, sem cifra, voltam tal como estão.
pub async fn decifrar(p: &SqlitePool, contexto: &str, valor: &str) -> AppResult<String> {
    let Some(resto) = valor.trim().strip_prefix(PREFIXO) else { return Ok(valor.to_string()) };
    let bytes = URL_SAFE_NO_PAD.decode(resto).map_err(|_| ilegivel(contexto))?;
    if bytes.len() < 12 + 16 {
        return Err(ilegivel(contexto));
    }
    let (n, c) = bytes.split_at(12);
    let n = Nonce::try_assume_unique_for_key(n).map_err(|_| ilegivel(contexto))?;
    let mut buf = c.to_vec();
    let claro = chave(p).await?.open_in_place(n, Aad::from(contexto.as_bytes()), &mut buf).map_err(|_| ilegivel(contexto))?;
    String::from_utf8(claro.to_vec()).map_err(|_| ilegivel(contexto))
}

/// Lê e abre um segredo da tabela `config`.
pub async fn ler_config(p: &SqlitePool, chave: &str) -> AppResult<Option<String>> {
    let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(chave).fetch_optional(p).await?;
    match v {
        Some(v) => Ok(Some(decifrar(p, &format!("config:{chave}"), &v).await?)),
        None => Ok(None),
    }
}

/// Valor cifrado para gravar na chave `chave` da tabela `config`.
pub async fn para_config(p: &SqlitePool, chave: &str, claro: &str) -> AppResult<String> {
    cifrar(p, &format!("config:{chave}"), claro).await
}

/// Cifra os segredos que ainda estão em texto (bases de versões anteriores). Devolve quantos cifrou.
pub async fn migrar(p: &SqlitePool) -> AppResult<usize> {
    let mut n = 0;
    for c in CHAVES_CONFIG {
        let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(c).fetch_optional(p).await?;
        if let Some(v) = v.filter(|v| !v.is_empty() && !v.trim().starts_with(PREFIXO)) {
            let cifrado = para_config(p, c, &v).await?;
            n += sqlx::query("UPDATE config SET valor = ? WHERE chave = ? AND valor = ?").bind(cifrado).bind(c).bind(&v).execute(p).await?.rows_affected() as usize;
        }
    }
    let webhooks: Vec<(i64, String)> = sqlx::query_as("SELECT id, segredo FROM api_webhooks WHERE segredo <> '' AND segredo NOT LIKE 'KGC1.%'").fetch_all(p).await?;
    for (id, s) in webhooks {
        let cifrado = cifrar(p, WEBHOOK, &s).await?;
        n += sqlx::query("UPDATE api_webhooks SET segredo = ? WHERE id = ? AND segredo = ?").bind(cifrado).bind(id).bind(&s).execute(p).await?.rows_affected() as usize;
    }
    Ok(n)
}
