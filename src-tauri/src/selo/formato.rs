//! Resumo do conteúdo de um executável para o selo de integridade. Partilhado com a ferramenta que sela na compilação
//! oficial (`scripts/selo`), por isso só usa a biblioteca padrão e o `sha2`.
//!
//! Conta o que o sistema carrega e executa: o conteúdo de todas as secções (código, constantes, dados), o endereço de
//! cada secção e o ponto de entrada. Não conta o resto dos cabeçalhos nem o fim do ficheiro, porque a assinatura de
//! código é acrescentada depois de compilar e só mexe aí (Authenticode no Windows; codesign no macOS, no segmento
//! `__LINKEDIT`, que não tem secções). Ao empacotar, o Tauri escreve no executável o tipo de pacote
//! (`__TAURI_BUNDLE_TYPE_VAR_…`): esses três caracteres contam sempre como `UNK`.
//!
//! Formatos: PE (Windows), Mach-O de 64 bits e universal (macOS) e ELF de 64 bits (Linux, para os testes).
//! Nenhuma leitura sai do ficheiro: um executável estragado dá um erro, nunca um pânico.

use sha2::{Digest, Sha256};

const DOMINIO: &[u8] = b"KanGest-selo-v1";
const MARCA_TAURI: &[u8] = b"__TAURI_BUNDLE_TYPE_VAR_";

/// Texto que a chave do produtor assina: versão, alvo da compilação e resumo em hexadecimal.
pub fn mensagem(versao: &str, alvo: &str, resumo_hex: &str) -> String {
    format!("KanGest-selo-v1\n{versao}\n{alvo}\n{resumo_hex}")
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Resumo SHA-256 do executável (ver o início do módulo).
pub fn resumo(executavel: &[u8]) -> Result<[u8; 32], String> {
    let mut b = executavel.to_vec();
    normalizar(&mut b);
    let partes = partes(&b)?;
    if !partes.iter().any(|p| p.tamanho > 0) {
        return Err("o executável não tem conteúdo para selar".into());
    }
    let mut h = Sha256::new();
    h.update(DOMINIO);
    for p in partes {
        let dados = ler(&b, p.inicio, p.tamanho).ok_or("uma secção sai fora do ficheiro")?;
        h.update((p.etiqueta.len() as u64).to_le_bytes());
        h.update(&p.etiqueta);
        h.update((p.tamanho as u64).to_le_bytes());
        h.update(dados);
    }
    Ok(h.finalize().into())
}

/// Onde estão as secções com conteúdo (início, tamanho): para os testes mexerem num byte que conta.
#[cfg(test)]
pub(crate) fn seccoes(b: &[u8]) -> Vec<(usize, usize)> {
    partes(b).map(|v| v.into_iter().filter(|p| p.tamanho > 0).map(|p| (p.inicio, p.tamanho)).collect()).unwrap_or_default()
}

/// Uma parte contada: etiqueta (nome, endereço, permissões) e conteúdo no ficheiro.
struct Parte {
    etiqueta: Vec<u8>,
    inicio: usize,
    tamanho: usize,
}

fn so_etiqueta(etiqueta: Vec<u8>) -> Parte {
    Parte { etiqueta, inicio: 0, tamanho: 0 }
}

fn normalizar(b: &mut [u8]) {
    let n = MARCA_TAURI.len();
    let mut i = 0;
    while i + n + 3 <= b.len() {
        let Some(p) = b[i..].iter().position(|&x| x == b'_') else { break };
        let j = i + p;
        if b.get(j..j + n) == Some(MARCA_TAURI) {
            if let Some(t) = b.get_mut(j + n..j + n + 3) {
                t.copy_from_slice(b"UNK");
            }
            i = j + n + 3;
        } else {
            i = j + 1;
        }
    }
}

fn ler(b: &[u8], o: usize, n: usize) -> Option<&[u8]> {
    b.get(o..o.checked_add(n)?)
}
fn u16le(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(ler(b, o, 2)?.try_into().ok()?))
}
fn u32le(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(ler(b, o, 4)?.try_into().ok()?))
}
fn u64le(b: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(ler(b, o, 8)?.try_into().ok()?))
}
fn u32be(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(ler(b, o, 4)?.try_into().ok()?))
}

fn partes(b: &[u8]) -> Result<Vec<Parte>, String> {
    let r = if b.starts_with(b"MZ") {
        pe(b)
    } else if b.starts_with(&[0xCF, 0xFA, 0xED, 0xFE]) {
        macho(b)
    } else if b.starts_with(&[0xCA, 0xFE, 0xBA, 0xBE]) {
        universal(b)
    } else if b.starts_with(b"\x7FELF") {
        elf(b)
    } else {
        return Err("formato de executável desconhecido".into());
    };
    r.ok_or_else(|| "cabeçalhos do executável ilegíveis".into())
}

/// Windows: as secções e o ponto de entrada.
fn pe(b: &[u8]) -> Option<Vec<Parte>> {
    let e = u32le(b, 0x3C)? as usize;
    if ler(b, e, 4)? != b"PE\0\0" {
        return None;
    }
    let n = u16le(b, e.checked_add(6)?)? as usize;
    let opcional = e.checked_add(24)?;
    let tam_opcional = u16le(b, e.checked_add(20)?)? as usize;
    let mut v = vec![so_etiqueta([b"PE-entrada".as_slice(), &u32le(b, opcional.checked_add(16)?)?.to_le_bytes()].concat())];
    let tabela = opcional.checked_add(tam_opcional)?;
    for i in 0..n {
        let s = tabela.checked_add(i.checked_mul(40)?)?;
        let cabeca = ler(b, s, 40)?;
        // nome, tamanho e endereço virtuais e características (não os ponteiros de relocalização)
        let etiqueta = [&cabeca[0..16], &cabeca[36..40]].concat();
        let tamanho = u32le(b, s + 16)? as usize;
        let inicio = u32le(b, s + 20)? as usize;
        v.push(Parte { etiqueta, inicio, tamanho });
    }
    Some(v)
}

/// macOS (64 bits): as secções de todos os segmentos (o `__LINKEDIT` não tem nenhuma) e o ponto de entrada.
fn macho(b: &[u8]) -> Option<Vec<Parte>> {
    let ncmds = u32le(b, 16)? as usize;
    let mut o = 32usize;
    let mut v = vec![so_etiqueta([b"MachO-cpu".as_slice(), ler(b, 4, 8)?].concat())];
    for _ in 0..ncmds {
        let cmd = u32le(b, o)?;
        let tam = u32le(b, o.checked_add(4)?)? as usize;
        if tam < 8 {
            return None;
        }
        match cmd {
            // LC_SEGMENT_64
            0x19 => {
                let nsects = u32le(b, o.checked_add(64)?)? as usize;
                for i in 0..nsects {
                    let s = o.checked_add(72)?.checked_add(i.checked_mul(80)?)?;
                    let cabeca = ler(b, s, 80)?;
                    // nome da secção e do segmento, endereço, tamanho e tipo (não os ponteiros de relocalização)
                    let etiqueta = [&cabeca[0..48], &cabeca[64..68]].concat();
                    let tamanho = u64le(b, s + 40)? as usize;
                    let tipo = u32le(b, s + 64)? & 0xFF;
                    // secções só em memória (S_ZEROFILL, S_GB_ZEROFILL, S_THREAD_LOCAL_ZEROFILL): não estão no ficheiro
                    if matches!(tipo, 0x01 | 0x0C | 0x12) {
                        v.push(so_etiqueta(etiqueta));
                    } else {
                        v.push(Parte { etiqueta, inicio: u32le(b, s + 48)? as usize, tamanho });
                    }
                }
            }
            // LC_MAIN: deslocamento do ponto de entrada
            0x8000_0028 => v.push(so_etiqueta([b"MachO-entrada".as_slice(), ler(b, o.checked_add(8)?, 8)?].concat())),
            _ => {}
        }
        o = o.checked_add(tam)?;
    }
    Some(v)
}

/// macOS universal: cada arquitectura, pela ordem do ficheiro.
fn universal(b: &[u8]) -> Option<Vec<Parte>> {
    let n = u32be(b, 4)? as usize;
    if n == 0 || n > 8 {
        return None;
    }
    let mut v = Vec::new();
    for i in 0..n {
        let a = 8 + i * 20;
        let inicio = u32be(b, a + 8)? as usize;
        let fatia = ler(b, inicio, u32be(b, a + 12)? as usize)?;
        if !fatia.starts_with(&[0xCF, 0xFA, 0xED, 0xFE]) {
            return None;
        }
        for p in macho(fatia)? {
            let inicio = if p.tamanho > 0 { p.inicio.checked_add(inicio)? } else { 0 };
            v.push(Parte { inicio, ..p });
        }
    }
    Some(v)
}

/// Linux (64 bits, para os testes): as secções carregadas em memória e o ponto de entrada.
fn elf(b: &[u8]) -> Option<Vec<Parte>> {
    if b.get(4) != Some(&2) || b.get(5) != Some(&1) {
        return None;
    }
    let mut v = vec![so_etiqueta([b"ELF-entrada".as_slice(), ler(b, 0x18, 8)?].concat())];
    let inicio_tabela = u64le(b, 0x28)? as usize;
    let tam_entrada = u16le(b, 0x3A)? as usize;
    let n = u16le(b, 0x3C)? as usize;
    if tam_entrada < 40 {
        return None;
    }
    for i in 0..n {
        let s = inicio_tabela.checked_add(i.checked_mul(tam_entrada)?)?;
        let tipo = u32le(b, s.checked_add(4)?)?;
        let flags = u64le(b, s + 8)?;
        // SHF_ALLOC: só o que vai para a memória
        if flags & 2 == 0 {
            continue;
        }
        let etiqueta = ler(b, s + 4, 20)?.to_vec();
        let tamanho = u64le(b, s + 32)? as usize;
        // SHT_NOBITS (.bss): só em memória
        if tipo == 8 {
            v.push(so_etiqueta(etiqueta));
        } else {
            v.push(Parte { etiqueta, inicio: u64le(b, s + 24)? as usize, tamanho });
        }
    }
    Some(v)
}

#[cfg(test)]
mod testes {
    use super::*;

    /// PE mínimo: cabeçalho DOS, PE, cabeçalho opcional de 240 bytes e duas secções.
    fn pe_teste(texto: &[u8], dados: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; 0x400];
        b[0..2].copy_from_slice(b"MZ");
        b[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        b[0x86..0x88].copy_from_slice(&2u16.to_le_bytes());
        b[0x94..0x96].copy_from_slice(&240u16.to_le_bytes());
        b[0x98 + 16..0x98 + 20].copy_from_slice(&0x1000u32.to_le_bytes()); // ponto de entrada
        let tabela = 0x98 + 240;
        for (i, (nome, conteudo, onde)) in [(b".text\0\0\0", texto, 0x200usize), (b".rdata\0\0", dados, 0x300)].iter().enumerate() {
            let s = tabela + i * 40;
            b[s..s + 8].copy_from_slice(*nome);
            b[s + 16..s + 20].copy_from_slice(&(conteudo.len() as u32).to_le_bytes());
            b[s + 20..s + 24].copy_from_slice(&(*onde as u32).to_le_bytes());
            b[*onde..*onde + conteudo.len()].copy_from_slice(conteudo);
        }
        b
    }

    #[test]
    fn pe_assinatura_e_tipo_de_pacote_nao_contam() {
        let original = pe_teste(b"\x55\x48\x89\xe5codigo", b"__TAURI_BUNDLE_TYPE_VAR_UNK\0chave publica");
        let r = resumo(&original).unwrap();
        // o Tauri marca o pacote (NSIS) e a assinatura Authenticode mexe no cabeçalho e acrescenta no fim
        let mut empacotado = original.clone();
        let p = empacotado.windows(3).position(|w| w == b"UNK").unwrap();
        empacotado[p..p + 3].copy_from_slice(b"NSS");
        empacotado[0x98 + 64..0x98 + 68].copy_from_slice(&0xDEADu32.to_le_bytes()); // CheckSum
        empacotado.extend_from_slice(&[0x30, 0x82, 0x01, 0x00, 1, 2, 3]); // certificado
        assert_eq!(resumo(&empacotado).unwrap(), r);
        // um byte de código ou de dados mexido, ou outro ponto de entrada, e o resumo muda
        let mut codigo = original.clone();
        codigo[0x201] ^= 1;
        assert_ne!(resumo(&codigo).unwrap(), r);
        let mut chave = original.clone();
        let p = chave.windows(5).position(|w| w == b"chave").unwrap();
        chave[p] = b'C';
        assert_ne!(resumo(&chave).unwrap(), r);
        let mut entrada = original.clone();
        entrada[0x98 + 16] = 0x20;
        assert_ne!(resumo(&entrada).unwrap(), r);
    }

    /// Mach-O mínimo: um LC_SEGMENT_64 com __text e __bss, LC_MAIN e uma assinatura no fim (LC_CODE_SIGNATURE).
    fn macho_teste(texto: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; 0x400];
        b[0..4].copy_from_slice(&[0xCF, 0xFA, 0xED, 0xFE]);
        b[4..8].copy_from_slice(&0x0100_000Cu32.to_le_bytes()); // arm64
        b[16..20].copy_from_slice(&3u32.to_le_bytes());
        let seg = 32;
        b[seg..seg + 4].copy_from_slice(&0x19u32.to_le_bytes());
        b[seg + 4..seg + 8].copy_from_slice(&(72u32 + 160).to_le_bytes());
        b[seg + 8..seg + 14].copy_from_slice(b"__TEXT");
        b[seg + 64..seg + 68].copy_from_slice(&2u32.to_le_bytes());
        let s = seg + 72;
        b[s..s + 6].copy_from_slice(b"__text");
        b[s + 16..s + 22].copy_from_slice(b"__TEXT");
        b[s + 40..s + 48].copy_from_slice(&(texto.len() as u64).to_le_bytes());
        b[s + 48..s + 52].copy_from_slice(&0x300u32.to_le_bytes());
        b[0x300..0x300 + texto.len()].copy_from_slice(texto);
        let s = s + 80;
        b[s..s + 5].copy_from_slice(b"__bss");
        b[s + 16..s + 22].copy_from_slice(b"__DATA");
        b[s + 40..s + 48].copy_from_slice(&4096u64.to_le_bytes());
        b[s + 64..s + 68].copy_from_slice(&1u32.to_le_bytes()); // S_ZEROFILL
        let main = seg + 72 + 160;
        b[main..main + 4].copy_from_slice(&0x8000_0028u32.to_le_bytes());
        b[main + 4..main + 8].copy_from_slice(&24u32.to_le_bytes());
        b[main + 8..main + 16].copy_from_slice(&0x300u64.to_le_bytes());
        let sig = main + 24;
        b[sig..sig + 4].copy_from_slice(&0x1Du32.to_le_bytes()); // LC_CODE_SIGNATURE
        b[sig + 4..sig + 8].copy_from_slice(&16u32.to_le_bytes());
        b
    }

    #[test]
    fn macho_e_universal() {
        let original = macho_teste(b"\xfd\x7b\xbf\xa9codigo arm64");
        let r = resumo(&original).unwrap();
        // o codesign muda a assinatura (no __LINKEDIT, no fim) e o tamanho dela no comando
        let mut assinado = original.clone();
        assinado[32 + 72 + 160 + 24 + 8..32 + 72 + 160 + 24 + 16].copy_from_slice(&[9, 9, 9, 9, 7, 7, 7, 7]);
        assinado.extend_from_slice(b"\xfa\xde\x0c\xc0 assinatura");
        assert_eq!(resumo(&assinado).unwrap(), r);
        let mut mexido = original.clone();
        mexido[0x305] ^= 0x80;
        assert_ne!(resumo(&mexido).unwrap(), r);

        // universal com uma arquitectura: o mesmo conteúdo, noutra posição do ficheiro
        let mut u = vec![0u8; 0x1000];
        u[0..4].copy_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE]);
        u[4..8].copy_from_slice(&1u32.to_be_bytes());
        u[16..20].copy_from_slice(&0x1000u32.to_be_bytes());
        u[20..24].copy_from_slice(&(original.len() as u32).to_be_bytes());
        u.extend_from_slice(&original);
        assert_eq!(resumo(&u).unwrap(), r, "o conteúdo é o mesmo");
    }

    #[test]
    fn ficheiros_estragados_dao_erro_sem_panico() {
        assert!(resumo(b"isto nao e um executavel").is_err());
        let pe = pe_teste(b"codigo", b"dados");
        for corte in [2, 0x40, 0x84, 0x98 + 240 + 10, 0x205] {
            assert!(resumo(&pe[..corte]).is_err(), "cortado em {corte}");
        }
        let mut gigante = pe.clone();
        gigante[0x98 + 240 + 16..0x98 + 240 + 20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(resumo(&gigante).is_err());
        let mut m = macho_teste(b"codigo");
        m[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(resumo(&m).is_err());
        let mut u = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 1];
        u.extend_from_slice(&[0, 0, 0, 7, 0, 0, 0, 3, 0xFF, 0xFF, 0xFF, 0xF0, 0, 0, 0, 0x40, 0, 0, 0, 12]);
        assert!(resumo(&u).is_err());
    }

    /// Um executável verdadeiro do sistema: PE no Windows, Mach-O universal no macOS, ELF no Linux.
    #[test]
    fn executavel_do_sistema() {
        let caminho = if cfg!(windows) { r"C:\Windows\System32\notepad.exe" } else { "/bin/ls" };
        let Ok(exe) = std::fs::read(caminho) else { return };
        let a = resumo(&exe).unwrap_or_else(|e| panic!("{caminho}: {e}"));
        assert_eq!(resumo(&exe).unwrap(), a);
        assert_eq!(hex(&a).len(), 64);
        // mexer num byte da maior secção (o código, em geral) muda o resumo
        let (inicio, tamanho) = seccoes(&exe).into_iter().max_by_key(|s| s.1).unwrap();
        let mut outro = exe.clone();
        outro[inicio + tamanho / 2] ^= 1;
        assert_ne!(resumo(&outro).unwrap(), a);
    }
}
