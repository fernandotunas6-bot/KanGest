// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Defesas do próprio programa contra alterações e engenharia reversa.
//!
//! - **Selo de integridade do executável.** Na compilação oficial (GitHub Actions), depois de compilar e antes de
//!   empacotar, a ferramenta `scripts/selo` calcula o resumo do executável ([`formato`]) e assina-o com a chave Ed25519
//!   do produtor; o selo vai com a aplicação como recurso (`selo.json`). Ao arrancar, o KanGest volta a calcular o
//!   resumo do seu executável e confirma a assinatura com a chave pública compilada nele. Um executável alterado (por
//!   exemplo, para saltar a licença) deixa de emitir documentos até ser reinstalado.
//! - **Depurador.** Nas compilações oficiais, com um depurador ligado ao processo não se emitem documentos.
//!
//! Limites (docs/SEGURANCA.md): quem altera o executável também pode retirar estas verificações. Elas obrigam a muito
//! mais trabalho, apanham alterações feitas às cegas, vírus que infectam executáveis e cópias corrompidas.

pub mod formato;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ring::signature::{UnparsedPublicKey, ED25519};

/// Chave pública do selo (Ed25519, base64url), posta pela compilação oficial. Sem ela a compilação não é oficial
/// (desenvolvimento ou compilada por outra pessoa): não há selo a confirmar.
const CHAVE_PUBLICA: Option<&str> = option_env!("KANGEST_SELO_PUBLICA");

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selo {
    /// Ainda a confirmar (nos primeiros instantes depois de arrancar).
    AConfirmar,
    /// Compilação sem selo (não oficial).
    SemSelo,
    Valido,
    Alterado(String),
}

impl Selo {
    pub fn codigo(&self) -> &'static str {
        match self {
            Selo::AConfirmar => "A_CONFIRMAR",
            Selo::SemSelo => "SEM_SELO",
            Selo::Valido => "VALIDO",
            Selo::Alterado(_) => "ALTERADO",
        }
    }
}

static ESTADO: OnceLock<Selo> = OnceLock::new();

/// Compilação oficial (com a chave pública do selo).
pub fn oficial() -> bool {
    CHAVE_PUBLICA.is_some_and(|k| !k.trim().is_empty())
}

/// Confirma o selo numa thread à parte (ler e resumir o executável leva alguns centésimos de segundo).
pub fn iniciar(recursos: PathBuf) {
    if !oficial() {
        let _ = ESTADO.set(Selo::SemSelo);
        return;
    }
    let _ = std::thread::Builder::new().name("kigest-selo".into()).spawn(move || {
        let r = match std::env::current_exe() {
            Ok(exe) => verificar(&exe, &recursos.join("selo.json"), CHAVE_PUBLICA.unwrap_or_default()),
            Err(e) => Selo::Alterado(format!("não foi possível ler o executável: {e}")),
        };
        let _ = ESTADO.set(r);
    });
}

pub fn estado() -> Selo {
    match ESTADO.get() {
        Some(s) => s.clone(),
        None if oficial() => Selo::AConfirmar,
        None => Selo::SemSelo,
    }
}

/// Confirma o selo de um executável. Público para os testes e para a ferramenta da compilação.
pub fn verificar(exe: &Path, selo: &Path, chave_publica: &str) -> Selo {
    let alterado = |m: &str| Selo::Alterado(m.to_string());
    let Ok(texto) = std::fs::read_to_string(selo) else { return alterado("falta o selo de integridade") };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&texto) else { return alterado("selo ilegível") };
    let (Some(versao), Some(alvo), Some(resumo), Some(assinatura)) = (v["versao"].as_str(), v["alvo"].as_str(), v["resumo"].as_str(), v["assinatura"].as_str()) else {
        return alterado("selo incompleto");
    };
    if versao != env!("CARGO_PKG_VERSION") {
        return Selo::Alterado(format!("o selo é da versão {versao}"));
    }
    let (Ok(chave), Ok(assinatura)) = (URL_SAFE_NO_PAD.decode(chave_publica.trim()), URL_SAFE_NO_PAD.decode(assinatura)) else {
        return alterado("selo ilegível");
    };
    if UnparsedPublicKey::new(&ED25519, &chave).verify(formato::mensagem(versao, alvo, resumo).as_bytes(), &assinatura).is_err() {
        return alterado("o selo não foi assinado pelo produtor");
    }
    let bytes = match std::fs::read(exe) {
        Ok(b) => b,
        Err(e) => return Selo::Alterado(format!("não foi possível ler o executável: {e}")),
    };
    match formato::resumo(&bytes) {
        Ok(r) if formato::hex(&r) == resumo => Selo::Valido,
        Ok(_) => alterado("o executável não corresponde ao selo"),
        Err(e) => Selo::Alterado(e),
    }
}

/// Há um depurador ligado a este processo?
pub fn depurador() -> bool {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn IsDebuggerPresent() -> i32;
        }
        // SAFETY: função do Windows sem argumentos, só lê o PEB do processo
        unsafe { IsDebuggerPresent() != 0 }
    }
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn sysctl(name: *mut i32, namelen: u32, oldp: *mut std::ffi::c_void, oldlenp: *mut usize, newp: *mut std::ffi::c_void, newlen: usize) -> i32;
        }
        // kinfo_proc do próprio processo (CTL_KERN, KERN_PROC, KERN_PROC_PID): kp_proc.p_flag está no deslocamento 32
        // e P_TRACED é 0x800 (nota técnica QA1361 da Apple)
        let mut info = [0u8; 1024];
        let mut tamanho = info.len();
        let mut nome = [1i32, 14, 1, std::process::id() as i32];
        // SAFETY: o tampão e o tamanho são válidos; o sistema escreve no máximo `tamanho` bytes
        let r = unsafe { sysctl(nome.as_mut_ptr(), 4, info.as_mut_ptr().cast(), &mut tamanho, std::ptr::null_mut(), 0) };
        r == 0 && tamanho >= 36 && i32::from_ne_bytes([info[32], info[33], info[34], info[35]]) & 0x800 != 0
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| s.lines().find_map(|l| l.strip_prefix("TracerPid:").map(|v| v.trim() != "0")))
            .unwrap_or(false)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        false
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    #[test]
    fn selo_do_executavel() {
        let pasta = std::env::temp_dir().join(format!("kigest-selo-{}", std::process::id()));
        std::fs::create_dir_all(&pasta).unwrap();
        // um executável pequeno (o /bin/ls, ou o notepad no Windows) faz de KanGest
        let origem = if cfg!(windows) { r"C:\Windows\System32\notepad.exe" } else { "/bin/ls" };
        let Ok(bytes) = std::fs::read(origem) else { return };
        let exe = pasta.join("kangest-teste");
        std::fs::write(&exe, &bytes).unwrap();

        let par = Ed25519KeyPair::from_seed_unchecked(&[7u8; 32]).unwrap();
        let publica = URL_SAFE_NO_PAD.encode(par.public_key().as_ref());
        let resumo = formato::hex(&formato::resumo(&bytes).unwrap());
        let versao = env!("CARGO_PKG_VERSION");
        let selar = |versao: &str, resumo: &str, par: &Ed25519KeyPair| {
            let assinatura = par.sign(formato::mensagem(versao, "teste", resumo).as_bytes());
            serde_json::json!({ "v": 1, "versao": versao, "alvo": "teste", "resumo": resumo, "assinatura": URL_SAFE_NO_PAD.encode(assinatura.as_ref()) }).to_string()
        };
        let selo = pasta.join("selo.json");
        std::fs::write(&selo, selar(versao, &resumo, &par)).unwrap();
        assert_eq!(verificar(&exe, &selo, &publica), Selo::Valido);

        // sem selo, selo de outra versão, assinado por outra chave
        assert!(matches!(verificar(&exe, &pasta.join("nao-existe.json"), &publica), Selo::Alterado(m) if m.contains("falta")));
        std::fs::write(&selo, selar("0.0.1", &resumo, &par)).unwrap();
        assert!(matches!(verificar(&exe, &selo, &publica), Selo::Alterado(m) if m.contains("versão")));
        let outro = Ed25519KeyPair::from_seed_unchecked(&[8u8; 32]).unwrap();
        std::fs::write(&selo, selar(versao, &resumo, &outro)).unwrap();
        assert!(matches!(verificar(&exe, &selo, &publica), Selo::Alterado(m) if m.contains("produtor")));

        // executável alterado depois de selado
        std::fs::write(&selo, selar(versao, &resumo, &par)).unwrap();
        let (inicio, tamanho) = formato::seccoes(&bytes).into_iter().max_by_key(|s| s.1).unwrap();
        let mut alterado = bytes.clone();
        alterado[inicio + tamanho / 2] ^= 0x5A;
        std::fs::write(&exe, &alterado).unwrap();
        assert!(matches!(verificar(&exe, &selo, &publica), Selo::Alterado(_)));
        let _ = std::fs::remove_dir_all(&pasta);
    }

    #[test]
    fn sem_depurador_nos_testes() {
        // a correr os testes normalmente, não há depurador
        assert!(!depurador());
    }
}
