// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Segredos guardados na configuração (tabela `config`): chave do contribuinte da Facturação
//! Electrónica, senhas e, nas compilações de desenvolvimento, a chave de assinatura.
//!
//! Ficam cifrados com AES-256-GCM. A chave mestra está num ficheiro à parte (`kangest.chave`, na
//! pasta de dados da aplicação), fora da base de dados: uma cópia da base, sozinha, não revela os
//! segredos. Restaurar a base noutra máquina obriga a introduzir de novo esses segredos.

use std::path::Path;
use std::sync::OnceLock;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

const PREFIXO: &str = "kgs1:";
const AAD: &[u8] = b"kangest-config";
pub const FICHEIRO: &str = "kangest.chave";

/// Chaves da tabela `config` que guardam segredos.
pub const CHAVES: [&str; 7] = ["agt.chave_privada", "fe.senha", "fe.chave_software", "fe.chave_contribuinte", "email.senha", "licenca.segredo", "web.token"];

static CHAVE: OnceLock<[u8; 32]> = OnceLock::new();

fn aleatorio<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    SystemRandom::new().fill(&mut b).expect("gerador aleatório do sistema");
    b
}

/// O cofre do sistema onde fica a chave mestra: Porta-chaves (macOS) e Gestor de Credenciais, protegido pela DPAPI
/// (Windows). Nos outros sistemas, e nos testes, a chave fica no ficheiro, como antes.
trait Cofre {
    /// `Ok(None)`: o cofre funciona mas não tem esta chave.
    fn ler(&self, id: &str) -> Result<Option<[u8; 32]>, String>;
    fn gravar(&self, id: &str, chave: &[u8; 32]) -> Result<(), String>;
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
struct CofreSistema;

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl Cofre for CofreSistema {
    fn ler(&self, id: &str) -> Result<Option<[u8; 32]>, String> {
        match keyring::Entry::new(SERVICO, id).map_err(|e| e.to_string())?.get_secret() {
            Ok(b) => b.try_into().map(Some).map_err(|_| "a chave no cofre está danificada".to_string()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn gravar(&self, id: &str, chave: &[u8; 32]) -> Result<(), String> {
        keyring::Entry::new(SERVICO, id).map_err(|e| e.to_string())?.set_secret(chave).map_err(|e| e.to_string())
    }
}

const SERVICO: &str = "ao.kigest.app";
/// O ficheiro `kangest.chave` guarda, em vez da chave, «cofre1:<identificador>» (mover a pasta de dados leva o
/// identificador consigo). Nas versões anteriores tinha os 32 bytes da chave.
const ID_PREFIXO: &str = "cofre1:";

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn escrever_ficheiro(f: &Path, conteudo: &[u8]) -> AppResult<()> {
    // escreve ao lado e troca, para um corte a meio nunca deixar o ficheiro pela metade
    let tmp = f.with_extension("chave.novo");
    std::fs::write(&tmp, conteudo).map_err(|e| AppError::msg(format!("Não foi possível criar {}: {e}", tmp.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, f).map_err(|e| AppError::msg(format!("Não foi possível gravar {}: {e}", f.display())))
}

/// Põe a chave no cofre e confirma lendo-a de volta; só então o ficheiro passa a guardar apenas o identificador.
fn guardar_no_cofre(cofre: &dyn Cofre, f: &Path, chave: &[u8; 32]) -> Result<(), String> {
    let id = hex(&aleatorio::<16>());
    cofre.gravar(&id, chave)?;
    if cofre.ler(&id)? != Some(*chave) {
        return Err("o cofre não devolveu a chave gravada".into());
    }
    escrever_ficheiro(f, format!("{ID_PREFIXO}{id}").as_bytes()).map_err(|e| e.to_string())
}

/// A chave mestra desta pasta de dados. Com cofre: nova instalação → chave nova no cofre; ficheiro antigo com a chave →
/// passa para o cofre (o ficheiro fica só com o identificador); ficheiro com identificador → lê do cofre. Se o cofre
/// falhar numa chave antiga, ela continua no ficheiro e o programa funciona como antes. `Ok(None)`: o cofre não deixa
/// ler a chave agora (bloqueado, ou o utilizador recusou o acesso): não se cria outra por cima.
fn obter_chave(pasta: &Path, cofre: Option<&dyn Cofre>) -> AppResult<Option<[u8; 32]>> {
    let f = pasta.join(FICHEIRO);
    let lido = match std::fs::read(&f) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(AppError::msg(format!("Não foi possível ler {}: {e}", f.display()))),
    };
    let nova = |pasta_f: &Path| -> AppResult<[u8; 32]> {
        let k = aleatorio::<32>();
        match cofre {
            Some(c) => {
                if let Err(motivo) = guardar_no_cofre(c, pasta_f, &k) {
                    eprintln!("Cofre do sistema indisponível ({motivo}): a chave mestra fica no ficheiro.");
                    escrever_ficheiro(pasta_f, &k)?;
                }
            }
            None => escrever_ficheiro(pasta_f, &k)?,
        }
        Ok(k)
    };
    let Some(b) = lido else { return nova(&f).map(Some) };
    if let Some(id) = std::str::from_utf8(&b).ok().and_then(|t| t.trim().strip_prefix(ID_PREFIXO)) {
        let Some(c) = cofre else {
            return Err(AppError::msg(format!("{} refere o cofre do sistema, que este computador não tem.", f.display())));
        };
        return match c.ler(id) {
            Ok(Some(k)) => Ok(Some(k)),
            // a chave desapareceu do cofre (outro utilizador do sistema, cofre reposto): recomeça-se, e os segredos
            // guardados voltam a pedir-se, como ao restaurar a base noutro computador
            Ok(None) => {
                eprintln!("A chave mestra não está no cofre do sistema: cria-se outra (os segredos guardados voltam a pedir-se).");
                let k = aleatorio::<32>();
                c.gravar(id, &k).map_err(|e| AppError::msg(format!("Não foi possível gravar a chave no cofre do sistema: {e}")))?;
                Ok(Some(k))
            }
            Err(motivo) => {
                eprintln!("Cofre do sistema: {motivo}");
                Ok(None)
            }
        };
    }
    let k: [u8; 32] = b.try_into().map_err(|_| AppError::msg(format!("O ficheiro {} está danificado.", f.display())))?;
    if let Some(c) = cofre {
        if let Err(motivo) = guardar_no_cofre(c, &f, &k) {
            eprintln!("Não foi possível passar a chave mestra para o cofre do sistema ({motivo}): fica no ficheiro.");
        }
    }
    Ok(Some(k))
}

/// Lê (ou cria, na primeira vez) a chave mestra desta pasta de dados.
pub fn iniciar(pasta: &Path) -> AppResult<()> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let cofre: Option<&dyn Cofre> = Some(&CofreSistema);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let cofre: Option<&dyn Cofre> = None;
    if let Some(chave) = obter_chave(pasta, cofre)? {
        let _ = CHAVE.set(chave);
    }
    Ok(())
}

/// Sem `iniciar` (testes), usa uma chave só desta execução.
fn chave() -> LessSafeKey {
    let k = CHAVE.get_or_init(aleatorio::<32>);
    LessSafeKey::new(UnboundKey::new(&AES_256_GCM, k).expect("chave AES-256"))
}

pub fn cifrar(texto: &str) -> String {
    let nonce = aleatorio::<NONCE_LEN>();
    let mut dados = texto.as_bytes().to_vec();
    chave()
        .seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(AAD), &mut dados)
        .expect("cifrar");
    let mut tudo = nonce.to_vec();
    tudo.extend_from_slice(&dados);
    format!("{PREFIXO}{}", B64.encode(tudo))
}

pub fn cifrado(valor: &str) -> bool {
    valor.starts_with(PREFIXO)
}

/// Decifra um valor guardado; um valor antigo (em texto) passa tal como está.
pub fn decifrar(valor: &str) -> Result<String, String> {
    let Some(b64) = valor.strip_prefix(PREFIXO) else { return Ok(valor.to_string()) };
    let tudo = B64.decode(b64.trim()).map_err(|_| "valor cifrado ilegível".to_string())?;
    if tudo.len() < NONCE_LEN + 16 {
        return Err("valor cifrado incompleto".into());
    }
    let (n, c) = tudo.split_at(NONCE_LEN);
    let nonce = Nonce::try_assume_unique_for_key(n).map_err(|_| "valor cifrado ilegível".to_string())?;
    let mut dados = c.to_vec();
    let texto = chave()
        .open_in_place(nonce, Aad::from(AAD), &mut dados)
        .map_err(|_| "foi cifrado noutra instalação (outra chave mestra)".to_string())?;
    String::from_utf8(texto.to_vec()).map_err(|_| "valor cifrado ilegível".to_string())
}

/// Lê um segredo da configuração.
pub async fn ler<'e, E: sqlx::SqliteExecutor<'e>>(e: E, chave: &str) -> AppResult<Option<String>> {
    let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(chave).fetch_optional(e).await?;
    match v {
        None => Ok(None),
        Some(v) => decifrar(&v).map(Some).map_err(|motivo| {
            AppError::msg(format!("O segredo «{chave}» não pode ser lido nesta máquina ({motivo}). Volte a introduzi-lo nas configurações."))
        }),
    }
}

/// Grava um segredo (cifrado) na configuração.
pub async fn gravar<'e, E: sqlx::SqliteExecutor<'e>>(e: E, chave: &str, valor: &str) -> AppResult<()> {
    sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
        .bind(chave)
        .bind(cifrar(valor))
        .execute(e)
        .await?;
    Ok(())
}

/// Cifra os segredos que versões anteriores gravaram em texto.
pub async fn cifrar_existentes(pool: &SqlitePool) -> AppResult<usize> {
    let mut n = 0;
    for k in CHAVES {
        let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(k).fetch_optional(pool).await?;
        if let Some(v) = v.filter(|v| !v.is_empty() && !cifrado(v)) {
            gravar(pool, k, &v).await?;
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod testes {
    #[test]
    fn ida_e_volta() {
        let c = super::cifrar("-----BEGIN PRIVATE KEY----- segredo");
        assert!(super::cifrado(&c) && !c.contains("segredo"));
        assert_eq!(super::decifrar(&c).unwrap(), "-----BEGIN PRIVATE KEY----- segredo");
        assert_ne!(super::cifrar("x"), super::cifrar("x"), "nonce diferente em cada vez");
        assert_eq!(super::decifrar("texto antigo").unwrap(), "texto antigo");
        let mut estragado = c.clone();
        estragado.replace_range(c.len() - 4.., "AAAA");
        assert!(super::decifrar(&estragado).is_err());
    }

    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// Um cofre falso: guarda em memória e pode falhar.
    #[derive(Default)]
    struct Falso {
        itens: RefCell<HashMap<String, [u8; 32]>>,
        falha_gravar: bool,
        falha_ler: bool,
    }
    impl Cofre for Falso {
        fn ler(&self, id: &str) -> Result<Option<[u8; 32]>, String> {
            if self.falha_ler {
                return Err("acesso recusado".into());
            }
            Ok(self.itens.borrow().get(id).copied())
        }
        fn gravar(&self, id: &str, chave: &[u8; 32]) -> Result<(), String> {
            if self.falha_gravar {
                return Err("cofre bloqueado".into());
            }
            self.itens.borrow_mut().insert(id.to_string(), *chave);
            Ok(())
        }
    }
    fn pasta(nome: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("kangest-cofre-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn instalacao_nova_poe_a_chave_no_cofre_e_so_o_identificador_no_ficheiro() {
        let p = pasta("nova");
        let c = Falso::default();
        let k = obter_chave(&p, Some(&c)).unwrap().unwrap();
        let f = std::fs::read_to_string(p.join(FICHEIRO)).unwrap();
        assert!(f.starts_with(ID_PREFIXO) && f.len() == ID_PREFIXO.len() + 32, "{f}");
        assert!(!f.contains(&hex(&k)), "a chave não está no ficheiro");
        assert_eq!(c.itens.borrow().len(), 1);
        // arranque seguinte: a mesma chave, vinda do cofre
        assert_eq!(obter_chave(&p, Some(&c)).unwrap(), Some(k));
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn ficheiro_antigo_com_a_chave_passa_para_o_cofre_sem_mudar_a_chave() {
        let p = pasta("antiga");
        let antiga = aleatorio::<32>();
        std::fs::write(p.join(FICHEIRO), antiga).unwrap();
        let c = Falso::default();
        assert_eq!(obter_chave(&p, Some(&c)).unwrap(), Some(antiga), "os segredos já cifrados continuam a abrir");
        assert!(std::fs::read(p.join(FICHEIRO)).unwrap().starts_with(ID_PREFIXO.as_bytes()));
        assert_eq!(c.itens.borrow().values().next(), Some(&antiga));
        assert_eq!(obter_chave(&p, Some(&c)).unwrap(), Some(antiga));
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn se_o_cofre_falhar_a_chave_antiga_fica_no_ficheiro() {
        let p = pasta("falha");
        let antiga = aleatorio::<32>();
        std::fs::write(p.join(FICHEIRO), antiga).unwrap();
        let c = Falso { falha_gravar: true, ..Default::default() };
        assert_eq!(obter_chave(&p, Some(&c)).unwrap(), Some(antiga));
        assert_eq!(std::fs::read(p.join(FICHEIRO)).unwrap(), antiga, "o ficheiro não foi tocado");
        // e uma instalação nova com o cofre avariado também funciona (chave no ficheiro)
        let p2 = pasta("falha2");
        let k = obter_chave(&p2, Some(&c)).unwrap().unwrap();
        assert_eq!(std::fs::read(p2.join(FICHEIRO)).unwrap(), k);
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::remove_dir_all(&p2);
    }

    #[test]
    fn cofre_que_recusa_ler_nao_cria_outra_chave() {
        let p = pasta("recusa");
        let c = Falso::default();
        obter_chave(&p, Some(&c)).unwrap();
        let antes = std::fs::read(p.join(FICHEIRO)).unwrap();
        let recusa = Falso { falha_ler: true, ..Default::default() };
        assert_eq!(obter_chave(&p, Some(&recusa)).unwrap(), None);
        assert_eq!(std::fs::read(p.join(FICHEIRO)).unwrap(), antes, "o identificador não muda");
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn chave_que_desapareceu_do_cofre_recomeca_com_o_mesmo_identificador() {
        let p = pasta("some");
        let c = Falso::default();
        let k1 = obter_chave(&p, Some(&c)).unwrap().unwrap();
        c.itens.borrow_mut().clear();
        let k2 = obter_chave(&p, Some(&c)).unwrap().unwrap();
        assert_ne!(k1, k2);
        assert_eq!(obter_chave(&p, Some(&c)).unwrap(), Some(k2));
        // sem cofre neste computador, um ficheiro com identificador é um erro claro
        assert!(obter_chave(&p, None).unwrap_err().to_string().contains("cofre do sistema"));
        let _ = std::fs::remove_dir_all(&p);
    }

    /// O Porta-chaves verdadeiro (só no macOS, e só quando se pede: `KANGEST_TESTE_COFRE=1`, pode pedir autorização).
    #[cfg(target_os = "macos")]
    #[test]
    fn porta_chaves_verdadeiro() {
        if std::env::var("KANGEST_TESTE_COFRE").is_err() {
            return;
        }
        let p = pasta("real");
        let antiga = aleatorio::<32>();
        std::fs::write(p.join(FICHEIRO), antiga).unwrap();
        assert_eq!(obter_chave(&p, Some(&CofreSistema)).unwrap(), Some(antiga));
        let f = std::fs::read_to_string(p.join(FICHEIRO)).unwrap();
        let id = f.strip_prefix(ID_PREFIXO).expect("passou para o Porta-chaves").to_string();
        assert_eq!(obter_chave(&p, Some(&CofreSistema)).unwrap(), Some(antiga), "lida de volta do Porta-chaves");
        keyring::Entry::new(SERVICO, &id).unwrap().delete_credential().unwrap();
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn sem_cofre_tudo_como_antes() {
        let p = pasta("sem");
        let k = obter_chave(&p, None).unwrap().unwrap();
        assert_eq!(std::fs::read(p.join(FICHEIRO)).unwrap(), k);
        assert_eq!(obter_chave(&p, None).unwrap(), Some(k));
        let _ = std::fs::remove_dir_all(&p);
    }
}
