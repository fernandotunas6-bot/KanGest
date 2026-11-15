//! Cópias de segurança (Fr_CopiaSeguranca do KIGEST): manual e automática diária.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

const MANTER: usize = 14;

/// Cópia consistente da base (mesmo com a aplicação em uso) via `VACUUM INTO`.
pub async fn copiar(pool: &SqlitePool, destino: &Path) -> AppResult<()> {
    if destino.exists() {
        std::fs::remove_file(destino).map_err(|e| AppError::msg(format!("Não foi possível substituir {}: {e}", destino.display())))?;
    }
    sqlx::query("VACUUM INTO ?").bind(destino.to_string_lossy().to_string()).execute(pool).await?;
    Ok(())
}

pub async fn backup(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.backup")?;
    let nome = format!("kangest-copia-{}.db", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    let destino = st.pasta_exportacao.join(nome);
    copiar(&st.pool, &destino).await?;
    let bytes = std::fs::metadata(&destino).map(|m| m.len()).unwrap_or(0);
    Ok(json!({ "ficheiro": destino.display().to_string(), "bytes": bytes }))
}

fn pasta_copias(pasta_dados: &Path) -> PathBuf {
    pasta_dados.join("copias")
}

/// Uma cópia por dia na pasta de dados; mantém as últimas 14.
pub async fn backup_automatico(pool: &SqlitePool, pasta_dados: &Path) -> AppResult<Option<PathBuf>> {
    let dir = pasta_copias(pasta_dados);
    std::fs::create_dir_all(&dir).map_err(|e| AppError::msg(e.to_string()))?;
    let destino = dir.join(format!("kangest-{}.db", chrono::Local::now().format("%Y%m%d")));
    if destino.exists() {
        return Ok(None);
    }
    copiar(pool, &destino).await?;
    // as mais antigas saem primeiro, pela data do ficheiro (os nomes mudaram de «kigest-» para «kangest-»)
    let mut ficheiros: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&dir)
        .map_err(|e| AppError::msg(e.to_string()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "db").unwrap_or(false))
        .map(|e| (e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), e.path()))
        .collect();
    ficheiros.sort();
    while ficheiros.len() > MANTER {
        let _ = std::fs::remove_file(ficheiros.remove(0).1);
    }
    Ok(Some(destino))
}

pub async fn copias(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.backup")?;
    let dir = pasta_copias(&st.pasta_dados);
    let mut lista: Vec<Value> = std::fs::read_dir(&dir)
        .map(|it| {
            it.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "db").unwrap_or(false))
                .map(|e| json!({ "ficheiro": e.path().display().to_string(), "bytes": e.metadata().map(|m| m.len()).unwrap_or(0) }))
                .collect()
        })
        .unwrap_or_default();
    lista.sort_by(|a, b| b["ficheiro"].as_str().cmp(&a["ficheiro"].as_str()));
    Ok(json!({ "pasta": dir.display().to_string(), "copias": lista }))
}

#[derive(serde::Deserialize)]
pub struct ExportarIn {
    nome: String,
    conteudo: String,
}

/// Grava um ficheiro de texto (CSV, mapas) na pasta de exportação e devolve o caminho.
/// O nome é limpo de separadores para não sair da pasta.
pub async fn exportar(st: &AppState, e: ExportarIn) -> AppResult<Value> {
    st.sessao()?;
    let nome: String = e
        .nome
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ') { c } else { '_' })
        .collect::<String>()
        .trim_matches(|c: char| c == '.' || c.is_whitespace())
        .to_string();
    if nome.is_empty() || !nome.contains('.') {
        return Err(AppError::msg("Nome de ficheiro inválido."));
    }
    let caminho = st.pasta_exportacao.join(&nome);
    // BOM UTF-8: o Excel abre os acentos correctamente
    let mut bytes = Vec::with_capacity(e.conteudo.len() + 3);
    if nome.to_lowercase().ends_with(".csv") {
        bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    bytes.extend_from_slice(e.conteudo.as_bytes());
    std::fs::write(&caminho, bytes).map_err(|err| AppError::msg(format!("Não foi possível gravar {}: {err}", caminho.display())))?;
    Ok(json!({ "ficheiro": caminho.display().to_string() }))
}

/// Ecrã «Sobre»: versão, pasta de dados, tamanho da base e contagens principais.
pub async fn info(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let base = st.pasta_dados.join("kigest.db");
    let bytes = std::fs::metadata(&base).map(|m| m.len()).unwrap_or(0);
    let conta = |sql: &'static str| async move { sqlx::query_scalar::<_, i64>(sql).fetch_one(&st.pool).await };
    let sqlite: String = sqlx::query_scalar("SELECT sqlite_version()").fetch_one(&st.pool).await?;
    let migracoes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations").fetch_one(&st.pool).await.unwrap_or(0);
    let ultima_copia = std::fs::read_dir(pasta_copias(&st.pasta_dados))
        .ok()
        .and_then(|it| it.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".db")).max());
    Ok(json!({
        "versao": env!("CARGO_PKG_VERSION"),
        "plataforma": format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        "sqlite": sqlite,
        "migracoes": migracoes,
        "pasta_dados": st.pasta_dados.display().to_string(),
        "pasta_exportacao": st.pasta_exportacao.display().to_string(),
        "base_bytes": bytes,
        "ultima_copia": ultima_copia,
        "artigos": conta("SELECT COUNT(*) FROM artigos").await?,
        "entidades": conta("SELECT COUNT(*) FROM entidades").await?,
        "documentos": conta("SELECT COUNT(*) FROM documentos").await?,
        "importados": conta("SELECT COUNT(*) FROM documentos WHERE importado = 1").await?,
        "utilizadores": conta("SELECT COUNT(*) FROM utilizadores").await?,
    }))
}

// ------------------------------------------------------------------ cópias cifradas com senha
//
// Ficheiro «.kgc»: `KGCOPIA1` + tamanho do cabeçalho (u32, little-endian) + cabeçalho JSON + blocos cifrados.
// A chave sai da senha por Argon2id (64 MiB, 3 passagens; os parâmetros vão no cabeçalho). Cada bloco de 1 MiB vai em
// AES-256-GCM com o número único = prefixo aleatório (7 bytes) + n.º do bloco (4) + 1 no último bloco (0 nos outros),
// e com o início do ficheiro (incluindo o cabeçalho) como dados autenticados: uma cópia cortada, com blocos trocados ou
// com um byte mexido não abre. Sem a senha, a cópia não se lê — nem com o KanGest.

const MAGIA: &[u8; 8] = b"KGCOPIA1";
const BLOCO: usize = 1 << 20;
const SENHA_MIN: usize = 8;

fn aleatorio<const N: usize>() -> AppResult<[u8; N]> {
    use ring::rand::SecureRandom;
    let mut b = [0u8; N];
    ring::rand::SystemRandom::new().fill(&mut b).map_err(|_| AppError::msg("Sem números aleatórios do sistema."))?;
    Ok(b)
}

fn chave_da_senha(senha: &str, sal: &[u8], m: u32, t: u32, p: u32) -> AppResult<ring::aead::LessSafeKey> {
    use argon2::{Algorithm, Argon2, Params, Version};
    let params = Params::new(m, t, p, Some(32)).map_err(|e| AppError::msg(format!("Parâmetros da cópia inválidos: {e}")))?;
    let mut k = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(senha.as_bytes(), sal, &mut k)
        .map_err(|e| AppError::msg(format!("Senha da cópia: {e}")))?;
    let k = ring::aead::UnboundKey::new(&ring::aead::AES_256_GCM, &k).map_err(|_| AppError::msg("Chave da cópia inválida."))?;
    Ok(ring::aead::LessSafeKey::new(k))
}

fn numero_unico(prefixo: &[u8; 7], i: u32, ultimo: bool) -> ring::aead::Nonce {
    let mut n = [0u8; 12];
    n[..7].copy_from_slice(prefixo);
    n[7..11].copy_from_slice(&i.to_be_bytes());
    n[11] = ultimo as u8;
    ring::aead::Nonce::assume_unique_for_key(n)
}

fn io(e: std::io::Error) -> AppError {
    AppError::msg(format!("Erro a ler ou gravar a cópia: {e}"))
}

/// Cifra o ficheiro `origem` (uma base SQLite) para `destino`, com a senha.
pub(crate) fn cifrar_copia(origem: &Path, destino: &Path, senha: &str) -> AppResult<()> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    use std::io::{BufReader, BufWriter, Read, Write};
    let (m, t, p) = (65_536u32, 3u32, 1u32);
    let sal: [u8; 16] = aleatorio()?;
    let prefixo: [u8; 7] = aleatorio()?;
    let cabecalho = json!({
        "v": 1, "kdf": { "alg": "argon2id", "m": m, "t": t, "p": p },
        "sal": B64.encode(sal), "prefixo": B64.encode(prefixo), "bloco": BLOCO,
        "versao": env!("CARGO_PKG_VERSION"), "criada_em": crate::db::agora(),
    })
    .to_string();
    let inicio = [MAGIA.as_slice(), &(cabecalho.len() as u32).to_le_bytes(), cabecalho.as_bytes()].concat();
    let chave = chave_da_senha(senha, &sal, m, t, p)?;
    let tamanho = std::fs::metadata(origem).map_err(io)?.len() as usize;
    let blocos = tamanho.div_ceil(BLOCO).max(1);
    let parcial = destino.with_extension("kgc.parcial");
    let r = (|| {
        let mut ent = BufReader::new(std::fs::File::open(origem).map_err(io)?);
        let mut sai = BufWriter::new(std::fs::File::create(&parcial).map_err(io)?);
        sai.write_all(&inicio).map_err(io)?;
        for i in 0..blocos {
            let ultimo = i + 1 == blocos;
            let mut dados = vec![0u8; if ultimo { tamanho - i * BLOCO } else { BLOCO }];
            ent.read_exact(&mut dados).map_err(io)?;
            chave
                .seal_in_place_append_tag(numero_unico(&prefixo, i as u32, ultimo), ring::aead::Aad::from(&inicio), &mut dados)
                .map_err(|_| AppError::msg("Não foi possível cifrar a cópia."))?;
            sai.write_all(&dados).map_err(io)?;
        }
        sai.into_inner().map_err(|e| io(e.into_error()))?.sync_all().map_err(io)?;
        std::fs::rename(&parcial, destino).map_err(io)
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(&parcial);
    }
    r
}

/// Abre uma cópia cifrada para `destino` e devolve o cabeçalho (versão, data).
pub(crate) fn decifrar_copia(origem: &Path, destino: &Path, senha: &str) -> AppResult<Value> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    use std::io::{BufRead, BufReader, BufWriter, Read, Write};
    let estragada = || AppError::msg("A cópia está estragada ou incompleta.");
    let mut ent = BufReader::new(std::fs::File::open(origem).map_err(|e| AppError::msg(format!("Não foi possível abrir {}: {e}", origem.display())))?);
    let mut magia = [0u8; 8];
    ent.read_exact(&mut magia).map_err(|_| estragada())?;
    if &magia != MAGIA {
        return Err(AppError::msg("Este ficheiro não é uma cópia cifrada do KanGest (.kgc)."));
    }
    let mut tam = [0u8; 4];
    ent.read_exact(&mut tam).map_err(|_| estragada())?;
    let tam_cab = u32::from_le_bytes(tam) as usize;
    if tam_cab > 64 * 1024 {
        return Err(estragada());
    }
    let mut cab = vec![0u8; tam_cab];
    ent.read_exact(&mut cab).map_err(|_| estragada())?;
    let v: Value = serde_json::from_slice(&cab).map_err(|_| estragada())?;
    if v["v"] != 1 || v["kdf"]["alg"] != "argon2id" {
        return Err(AppError::msg("Cópia de um formato que esta versão não conhece. Actualize o KanGest."));
    }
    let num = |x: &Value| x.as_u64().and_then(|n| u32::try_from(n).ok());
    let (Some(m), Some(t), Some(p), Some(bloco)) = (num(&v["kdf"]["m"]), num(&v["kdf"]["t"]), num(&v["kdf"]["p"]), num(&v["bloco"])) else {
        return Err(estragada());
    };
    // limites: um ficheiro feito à mão não pode pôr o computador a calcular durante horas
    if !(8_192..=1_048_576).contains(&m) || !(1..=10).contains(&t) || !(1..=4).contains(&p) || !(4_096..=16_777_216).contains(&bloco) {
        return Err(estragada());
    }
    let sal = B64.decode(v["sal"].as_str().unwrap_or_default()).map_err(|_| estragada())?;
    let prefixo: [u8; 7] = B64.decode(v["prefixo"].as_str().unwrap_or_default()).ok().and_then(|p| p.try_into().ok()).ok_or_else(estragada)?;
    let inicio = [MAGIA.as_slice(), &tam, &cab].concat();
    let chave = chave_da_senha(senha, &sal, m, t, p)?;
    let parcial = destino.with_extension("parcial");
    let r = (|| {
        let mut sai = BufWriter::new(std::fs::File::create(&parcial).map_err(io)?);
        let mut buf = vec![0u8; bloco as usize + 16];
        let mut i: u32 = 0;
        loop {
            // um bloco inteiro (ou o que resta do ficheiro)
            let mut n = 0;
            while n < buf.len() {
                match ent.read(&mut buf[n..]).map_err(io)? {
                    0 => break,
                    k => n += k,
                }
            }
            let ultimo = n < buf.len() || ent.fill_buf().map_err(io)?.is_empty();
            if n < 16 {
                return Err(estragada());
            }
            let claro = chave
                .open_in_place(numero_unico(&prefixo, i, ultimo), ring::aead::Aad::from(&inicio), &mut buf[..n])
                .map_err(|_| if i == 0 { AppError::msg("Senha errada (ou a cópia está estragada).") } else { estragada() })?;
            sai.write_all(claro).map_err(io)?;
            if ultimo {
                break;
            }
            i = i.checked_add(1).ok_or_else(estragada)?;
        }
        sai.into_inner().map_err(|e| io(e.into_error()))?.sync_all().map_err(io)?;
        std::fs::rename(&parcial, destino).map_err(io)
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(&parcial);
    }
    r.map(|_| json!({ "versao": v["versao"], "criada_em": v["criada_em"] }))
}

#[derive(serde::Deserialize)]
pub struct CopiaCifradaIn {
    senha: String,
}

/// Cópia de segurança cifrada com uma senha, na pasta de exportação (para levar numa pen ou guardar na nuvem).
pub async fn backup_cifrado(st: &AppState, i: CopiaCifradaIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.backup")?;
    if i.senha.chars().count() < SENHA_MIN {
        return Err(AppError::msg(format!("A senha da cópia tem de ter pelo menos {SENHA_MIN} caracteres.")));
    }
    let agora = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let temporaria = st.pasta_dados.join(format!("copia-a-cifrar-{agora}.db"));
    let destino = st.pasta_exportacao.join(format!("kangest-copia-{agora}.kgc"));
    copiar(&st.pool, &temporaria).await?;
    let (t, d) = (temporaria.clone(), destino.clone());
    let r = tokio::task::spawn_blocking(move || cifrar_copia(&t, &d, &i.senha)).await.map_err(|e| AppError::msg(e.to_string()));
    let _ = std::fs::remove_file(&temporaria);
    r??;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'COPIA', ?)")
        .bind(s.id)
        .bind(format!("Cópia de segurança cifrada: {}", destino.display()))
        .execute(&st.pool)
        .await?;
    let bytes = std::fs::metadata(&destino).map(|m| m.len()).unwrap_or(0);
    Ok(json!({ "ficheiro": destino.display().to_string(), "bytes": bytes }))
}

/// Cópias que se podem restaurar: as cifradas (.kgc) e as manuais (.db) à mão, e as automáticas.
pub async fn copias_restaurar(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.backup")?;
    let pasta_exp = st.pasta_exportacao.clone();
    let mut lista = tokio::task::spawn_blocking(move || super::importar::procurar_ficheiros(&pasta_exp, &["kgc", "db"]))
        .await
        .map_err(|e| AppError::msg(e.to_string()))?;
    // das .db, só as cópias do KanGest (não outras bases que estejam por ali)
    lista.retain(|f| {
        let nome = f["nome"].as_str().unwrap_or_default().to_lowercase();
        nome.ends_with(".kgc") || nome.starts_with("kangest-") || nome.starts_with("kigest-")
    });
    for e in std::fs::read_dir(pasta_copias(&st.pasta_dados)).into_iter().flatten().flatten() {
        let nome = e.file_name().to_string_lossy().to_string();
        if nome.ends_with(".db") {
            let meta = e.metadata().ok();
            lista.push(json!({
                "caminho": e.path().display().to_string(), "nome": nome, "tamanho": meta.as_ref().map(|m| m.len()).unwrap_or(0),
                "modificado": meta.and_then(|m| m.modified().ok()).map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%dT%H:%M:%S").to_string()).unwrap_or_default(),
            }));
        }
    }
    lista.sort_by(|a, b| b["modificado"].as_str().cmp(&a["modificado"].as_str()));
    Ok(json!({ "copias": lista, "pendente": pendente(&st.pasta_dados).await }))
}

fn restauro(pasta_dados: &Path) -> PathBuf {
    pasta_dados.join("kigest.restaurar.db")
}

/// Restauro preparado e à espera de reabrir o programa: empresa e documentos da cópia.
async fn pendente(pasta_dados: &Path) -> Value {
    let f = restauro(pasta_dados);
    if !f.exists() {
        return Value::Null;
    }
    validar_base(&f).await.unwrap_or(Value::Null)
}

/// Confirma que um ficheiro é uma base do KanGest inteira e que esta versão a sabe abrir.
async fn validar_base(caminho: &Path) -> AppResult<Value> {
    use sqlx::Connection;
    let nao_e = || AppError::msg("O ficheiro não é uma base de dados do KanGest.");
    let opts = sqlx::sqlite::SqliteConnectOptions::new().filename(caminho);
    let mut c = sqlx::SqliteConnection::connect_with(&opts).await.map_err(|_| nao_e())?;
    let r = async {
        let integridade: String = sqlx::query_scalar("PRAGMA integrity_check").fetch_one(&mut c).await.map_err(|_| nao_e())?;
        if integridade != "ok" {
            return Err(AppError::msg("A cópia está estragada (a verificação de integridade da base falhou)."));
        }
        let empresa: Option<(String, String)> = sqlx::query_as("SELECT nome, nif FROM empresa WHERE id = 1").fetch_optional(&mut c).await.map_err(|_| nao_e())?;
        let migracao: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations").fetch_one(&mut c).await.map_err(|_| nao_e())?;
        if migracao > crate::db::ultima_migracao() {
            return Err(AppError::msg("A cópia é de uma versão mais recente do KanGest. Actualize este KanGest antes de a restaurar."));
        }
        let documentos: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documentos").fetch_one(&mut c).await.map_err(|_| nao_e())?;
        let ultimo: Option<String> = sqlx::query_scalar("SELECT MAX(data_hora) FROM documentos").fetch_one(&mut c).await.map_err(|_| nao_e())?;
        Ok(json!({
            "empresa": empresa.as_ref().map(|e| e.0.clone()), "nif": empresa.map(|e| e.1),
            "documentos": documentos, "ultimo_documento": ultimo,
        }))
    }
    .await;
    let _ = c.close().await;
    r
}

#[derive(serde::Deserialize)]
pub struct RestaurarIn {
    caminho: String,
    #[serde(default)]
    senha: Option<String>,
}

/// Prepara o restauro de uma cópia: abre-a (com a senha, se for cifrada), confirma que é uma base do KanGest inteira e
/// deixa-a pronta. A troca faz-se ao reabrir o programa ([`aplicar_restauro`]); a base actual fica guardada nas cópias.
pub async fn restaurar(st: &AppState, i: RestaurarIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.backup")?;
    st.exigir_perm("sistema.configuracoes")?;
    let origem = PathBuf::from(i.caminho.trim());
    if !origem.is_file() {
        return Err(AppError::msg("Ficheiro não encontrado."));
    }
    let destino = restauro(&st.pasta_dados);
    let preparado = st.pasta_dados.join("kigest.restaurar.parcial.db");
    let _ = std::fs::remove_file(&preparado);
    let cifrada = origem.extension().is_some_and(|x| x.eq_ignore_ascii_case("kgc"));
    let cabecalho = if cifrada {
        let senha = i.senha.unwrap_or_default();
        if senha.is_empty() {
            return Err(AppError::msg("Esta cópia é cifrada: indique a senha."));
        }
        let (o, d) = (origem.clone(), preparado.clone());
        tokio::task::spawn_blocking(move || decifrar_copia(&o, &d, &senha)).await.map_err(|e| AppError::msg(e.to_string()))??
    } else {
        std::fs::copy(&origem, &preparado).map_err(|e| AppError::msg(format!("Não foi possível copiar {}: {e}", origem.display())))?;
        Value::Null
    };
    let info = match validar_base(&preparado).await {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::remove_file(&preparado);
            return Err(e);
        }
    };
    for f in [destino.clone(), PathBuf::from(format!("{}-wal", destino.display())), PathBuf::from(format!("{}-shm", destino.display()))] {
        let _ = std::fs::remove_file(f);
    }
    std::fs::rename(&preparado, &destino).map_err(|e| AppError::msg(format!("Não foi possível preparar o restauro: {e}")))?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'COPIA', ?)")
        .bind(s.id)
        .bind(format!("Restauro preparado a partir de {} (fica feito ao reabrir o programa)", origem.display()))
        .execute(&st.pool)
        .await?;
    Ok(json!({ "copia": info, "cabecalho": cabecalho }))
}

/// Desiste de um restauro preparado.
pub async fn restaurar_cancelar(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.backup")?;
    let _ = std::fs::remove_file(restauro(&st.pasta_dados));
    Ok(Value::Null)
}

/// Ao arrancar, antes de abrir a base: troca-a pela cópia preparada. A base que estava em uso fica nas cópias
/// automáticas (`kangest-antes-do-restauro-….db`, com o que estivesse ainda no registo WAL). Devolve essa cópia.
pub async fn aplicar_restauro(pasta_dados: &Path) -> AppResult<Option<PathBuf>> {
    use sqlx::Connection;
    let novo = restauro(pasta_dados);
    if !novo.exists() {
        return Ok(None);
    }
    let base = pasta_dados.join("kigest.db");
    let dir = pasta_copias(pasta_dados);
    std::fs::create_dir_all(&dir).map_err(|e| AppError::msg(e.to_string()))?;
    let antes = dir.join(format!("kangest-antes-do-restauro-{}.db", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    if base.exists() {
        // VACUUM INTO junta o registo WAL: a cópia da base antiga fica completa num só ficheiro
        let vacuum = async {
            let mut c = sqlx::SqliteConnection::connect_with(&sqlx::sqlite::SqliteConnectOptions::new().filename(&base)).await?;
            let r = sqlx::query("VACUUM INTO ?").bind(antes.to_string_lossy().to_string()).execute(&mut c).await;
            let _ = c.close().await;
            r.map(|_| ())
        };
        if vacuum.await.is_ok() {
            for f in ["kigest.db", "kigest.db-wal", "kigest.db-shm"] {
                let f = pasta_dados.join(f);
                if f.exists() {
                    std::fs::remove_file(&f).map_err(|e| AppError::msg(format!("Não foi possível substituir {}: {e}", f.display())))?;
                }
            }
        } else {
            // base estragada (talvez a razão do restauro): guarda os ficheiros como estão
            let _ = std::fs::remove_file(&antes);
            for sufixo in ["", "-wal", "-shm"] {
                let f = PathBuf::from(format!("{}{sufixo}", base.display()));
                if f.exists() {
                    std::fs::rename(&f, PathBuf::from(format!("{}{sufixo}", antes.display())))
                        .map_err(|e| AppError::msg(format!("Não foi possível guardar {}: {e}", f.display())))?;
                }
            }
        }
    }
    std::fs::rename(&novo, &base).map_err(|e| AppError::msg(format!("Não foi possível restaurar a cópia: {e}")))?;
    for sufixo in ["-wal", "-shm"] {
        let f = PathBuf::from(format!("{}{sufixo}", novo.display()));
        if f.exists() {
            let _ = std::fs::rename(&f, PathBuf::from(format!("{}{sufixo}", base.display())));
        }
    }
    Ok(Some(antes))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pasta(nome: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("kigest-copia-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn copia_cifrada_abre_so_com_a_senha_e_inteira() {
        let d = pasta("cifra");
        // 2,5 blocos com conteúdo variado; um bloco exacto; ficheiro vazio
        let conteudos: Vec<Vec<u8>> = vec![(0..BLOCO * 5 / 2).map(|i| (i * 31 % 251) as u8).collect(), vec![7u8; BLOCO], Vec::new()];
        for (n, conteudo) in conteudos.iter().enumerate() {
            let (claro, cifrada, aberta) = (d.join(format!("{n}.db")), d.join(format!("{n}.kgc")), d.join(format!("{n}.aberta.db")));
            std::fs::write(&claro, conteudo).unwrap();
            cifrar_copia(&claro, &cifrada, "senha-boa-123").unwrap();
            let bytes = std::fs::read(&cifrada).unwrap();
            assert!(bytes.starts_with(MAGIA));
            if conteudo.len() > 64 {
                assert!(!bytes.windows(64).any(|w| w == &conteudo[..64]), "o conteúdo não aparece em claro");
            }
            let cab = decifrar_copia(&cifrada, &aberta, "senha-boa-123").unwrap();
            assert_eq!(cab["versao"], env!("CARGO_PKG_VERSION"));
            assert_eq!(&std::fs::read(&aberta).unwrap(), conteudo);
        }

        let cifrada = d.join("0.kgc");
        let bytes = std::fs::read(&cifrada).unwrap();
        let destino = d.join("x.db");
        let erro = |b: &[u8], senha: &str| {
            let f = d.join("mexida.kgc");
            std::fs::write(&f, b).unwrap();
            let e = decifrar_copia(&f, &destino, senha).unwrap_err().to_string();
            assert!(!destino.exists(), "nada fica escrito quando a cópia não abre");
            e
        };
        assert!(erro(&bytes, "senha-errada").contains("Senha errada"));
        // cortada no fim de um bloco: o que fica não tem a marca de último bloco
        let inicio = 12 + u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        assert!(erro(&bytes[..inicio + 2 * (BLOCO + 16)], "senha-boa-123").contains("estragada"));
        assert!(erro(&bytes[..bytes.len() - 1], "senha-boa-123").contains("estragada"));
        // um byte mexido no meio, blocos trocados, cabeçalho alterado
        let mut mexida = bytes.clone();
        mexida[inicio + BLOCO + 100] ^= 1;
        assert!(erro(&mexida, "senha-boa-123").contains("estragada"));
        let mut trocada = bytes.clone();
        let (a, b) = (inicio, inicio + BLOCO + 16);
        let bloco0 = trocada[a..b].to_vec();
        let bloco1 = trocada[b..b + BLOCO + 16].to_vec();
        trocada[a..b].copy_from_slice(&bloco1);
        trocada[b..b + BLOCO + 16].copy_from_slice(&bloco0);
        assert!(!erro(&trocada, "senha-boa-123").is_empty());
        let texto = String::from_utf8_lossy(&bytes[12..inicio]).replace(env!("CARGO_PKG_VERSION"), "9.9.9");
        let mut cabecalho = bytes[..12].to_vec();
        cabecalho.extend_from_slice(texto.as_bytes());
        cabecalho.extend_from_slice(&bytes[inicio..]);
        assert!(!erro(&cabecalho, "senha-boa-123").is_empty());
        // parâmetros absurdos num ficheiro feito à mão não põem o computador a calcular
        let texto = String::from_utf8_lossy(&bytes[12..inicio]).replace("\"m\":65536", "\"m\":4194304");
        let mut gulosa = MAGIA.to_vec();
        gulosa.extend_from_slice(&(texto.len() as u32).to_le_bytes());
        gulosa.extend_from_slice(texto.as_bytes());
        gulosa.extend_from_slice(&bytes[inicio..]);
        assert!(erro(&gulosa, "senha-boa-123").contains("estragada"));
        assert!(erro(b"SQLite format 3\0", "x").contains("não é uma cópia cifrada"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
