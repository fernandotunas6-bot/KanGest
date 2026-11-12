# Exportador de dados do KIGEST para o KanGest
#
# Corre no computador onde está a base do KIGEST (SQL Server, base KigestGC): lê o Conexao.ini,
# liga-se à base e grava todas as tabelas num ficheiro «.kigest» (JSON por linhas, comprimido).
# No KanGest: Sistema → Importar dados do KIGEST → Ficheiro exportado.
#
# Não altera nada na base do KIGEST (só leitura). As colunas de senhas não são exportadas.
# Requisitos: Windows 7 SP1 ou mais recente com o PowerShell 5 e o .NET Framework 4.5 (já vêm no Windows 10/11).
#
# Uso: duplo clique em ExportarKIGEST.bat, ou
#   powershell -ExecutionPolicy Bypass -File ExportarKIGEST.ps1 [-Conexao C:\KIGEST GC\Conexao.ini] [-Servidor NOME\INSTANCIA] [-Destino C:\pasta]

param(
    [string]$Conexao = "",
    [string]$Servidor = "",
    [string]$Destino = ""
)

$ErrorActionPreference = "Stop"
try { $Host.UI.RawUI.WindowTitle = "KanGest - Exportar dados do KIGEST" } catch { }

# Vistas que o KanGest usa (as tabelas exportam-se todas).
$Vistas = @(
    "vsBancosContas", "vsClientes", "vsDocsProformaCabessalho", "vsDocsProformaDetalhes", "vsFornecedores",
    "vsOficinaFichaAutomovel", "vsOrdemServicoCabessalho", "vsOrdemServicoDetalhes", "vsProformaDetalhes",
    "vsRhProcesamentoHonorarios", "vsSAFT_REC_Cabecalho", "vsSAFT_ReciboLinha", "vsTotalporPagtoRecibos"
)

$Codigo = @'
using System;
using System.Collections.Generic;
using System.Data;
using System.Data.SqlClient;
using System.Globalization;
using System.IO;
using System.IO.Compression;
using System.Text;

public static class KigestExportador
{
    public const int LimiteBinario = 8 * 1024 * 1024;

    public static string Json(string s)
    {
        StringBuilder sb = new StringBuilder(s.Length + 2);
        sb.Append('"');
        foreach (char c in s)
        {
            switch (c)
            {
                case '"': sb.Append("\\\""); break;
                case '\\': sb.Append("\\\\"); break;
                case '\n': sb.Append("\\n"); break;
                case '\r': sb.Append("\\r"); break;
                case '\t': sb.Append("\\t"); break;
                default:
                    if (c < ' ') sb.Append("\\u").Append(((int)c).ToString("x4"));
                    else sb.Append(c);
                    break;
            }
        }
        sb.Append('"');
        return sb.ToString();
    }

    static bool Sensivel(string coluna)
    {
        string c = coluna.ToLowerInvariant();
        return c.Contains("senha") || c.Contains("password") || c == "pwd" || c == "pass";
    }

    static string Numero(double d)
    {
        if (double.IsNaN(d) || double.IsInfinity(d)) return "null";
        return d.ToString("R", CultureInfo.InvariantCulture);
    }

    static string Valor(SqlDataReader r, int i, string tipoSql)
    {
        if (r.IsDBNull(i)) return "null";
        object v = r.GetValue(i);
        if (v is bool) return (bool)v ? "true" : "false";
        if (v is byte || v is short || v is int || v is long) return Convert.ToInt64(v).ToString(CultureInfo.InvariantCulture);
        if (v is decimal) return ((decimal)v).ToString(CultureInfo.InvariantCulture);
        if (v is double) return Numero((double)v);
        if (v is float) return Numero((double)(float)v);
        if (v is DateTime)
        {
            DateTime d = (DateTime)v;
            return Json(tipoSql == "date" ? d.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture) : d.ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture));
        }
        if (v is DateTimeOffset) return Json(((DateTimeOffset)v).ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture));
        if (v is TimeSpan) return Json(((TimeSpan)v).ToString("c", CultureInfo.InvariantCulture));
        if (v is byte[])
        {
            byte[] b = (byte[])v;
            if (b.Length > LimiteBinario) return "null";
            return "{\"b64\":\"" + Convert.ToBase64String(b) + "\"}";
        }
        return Json(Convert.ToString(v, CultureInfo.InvariantCulture));
    }

    // Devolve: [0] n.º de objectos, [1] n.º de registos, [2..] avisos
    public static string[] Exportar(string ligacao, string origem, string ficheiro, string[] vistas)
    {
        List<string> avisos = new List<string>();
        List<string[]> objectos = new List<string[]>();
        HashSet<string> quero = new HashSet<string>(vistas, StringComparer.OrdinalIgnoreCase);
        long total = 0;
        int n = 0;
        using (SqlConnection cn = new SqlConnection(ligacao))
        {
            cn.Open();
            using (SqlCommand cmd = new SqlCommand("SELECT s.name, o.name, o.type FROM sys.objects o JOIN sys.schemas s ON s.schema_id = o.schema_id WHERE (o.type = 'U' AND o.is_ms_shipped = 0) OR o.type = 'V' ORDER BY o.type, o.name", cn))
            using (SqlDataReader r = cmd.ExecuteReader())
            {
                while (r.Read())
                {
                    string tipo = r.GetString(2).Trim();
                    if (tipo == "V" && !quero.Contains(r.GetString(1))) continue;
                    objectos.Add(new string[] { r.GetString(0), r.GetString(1), tipo });
                }
            }

            using (FileStream fs = new FileStream(ficheiro, FileMode.Create, FileAccess.Write))
            using (GZipStream gz = new GZipStream(fs, CompressionMode.Compress))
            using (StreamWriter w = new StreamWriter(gz, new UTF8Encoding(false), 1 << 16))
            {
                w.NewLine = "\n";
                w.WriteLine("{\"formato\":\"kigest-exportacao\",\"versao\":1,\"origem\":" + Json(origem)
                    + ",\"data\":" + Json(DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture))
                    + ",\"computador\":" + Json(Environment.MachineName) + ",\"objectos\":" + objectos.Count + "}");
                foreach (string[] o in objectos)
                {
                    string nome = o[1];
                    Console.Write("  " + nome.PadRight(44));
                    long linhas = 0;
                    try
                    {
                        string sql = "SELECT * FROM [" + o[0].Replace("]", "]]") + "].[" + nome.Replace("]", "]]") + "]";
                        using (SqlCommand cmd = new SqlCommand(sql, cn))
                        {
                            cmd.CommandTimeout = 0;
                            using (SqlDataReader r = cmd.ExecuteReader())
                            {
                                int nc = r.FieldCount;
                                string[] tipos = new string[nc];
                                bool[] sens = new bool[nc];
                                StringBuilder sb = new StringBuilder();
                                sb.Append("{\"tabela\":").Append(Json(nome)).Append(",\"tipo\":").Append(o[2] == "V" ? "\"VISTA\"" : "\"TABELA\"").Append(",\"colunas\":[");
                                for (int i = 0; i < nc; i++)
                                {
                                    string col = r.GetName(i);
                                    tipos[i] = r.GetDataTypeName(i).ToLowerInvariant();
                                    sens[i] = Sensivel(col);
                                    if (i > 0) sb.Append(',');
                                    sb.Append(Json(col));
                                }
                                sb.Append("]}");
                                w.WriteLine(sb.ToString());
                                while (r.Read())
                                {
                                    sb.Length = 0;
                                    sb.Append('[');
                                    for (int i = 0; i < nc; i++)
                                    {
                                        if (i > 0) sb.Append(',');
                                        sb.Append(sens[i] ? "null" : Valor(r, i, tipos[i]));
                                    }
                                    sb.Append(']');
                                    w.WriteLine(sb.ToString());
                                    linhas++;
                                }
                            }
                        }
                        w.WriteLine("{\"fim\":" + Json(nome) + ",\"registos\":" + linhas + "}");
                        Console.WriteLine(linhas.ToString("N0", CultureInfo.CurrentCulture) + " registos");
                    }
                    catch (Exception e)
                    {
                        w.WriteLine("{\"fim\":" + Json(nome) + ",\"registos\":" + linhas + ",\"erro\":" + Json(e.Message) + "}");
                        Console.WriteLine("ERRO: " + e.Message);
                        avisos.Add(nome + ": " + e.Message);
                    }
                    total += linhas;
                    n++;
                }
                w.WriteLine("{\"concluido\":true,\"objectos\":" + n + ",\"registos\":" + total + "}");
            }
        }
        List<string> res = new List<string>();
        res.Add(n.ToString(CultureInfo.InvariantCulture));
        res.Add(total.ToString(CultureInfo.InvariantCulture));
        res.AddRange(avisos);
        return res.ToArray();
    }
}
'@

function Fim([int]$codigo) {
    Write-Host ""
    Read-Host "Prima Enter para fechar" | Out-Null
    exit $codigo
}

function Ler-Ini([string]$caminho) {
    $v = @{}
    foreach ($linha in [System.IO.File]::ReadAllLines($caminho, [System.Text.Encoding]::Default)) {
        $t = $linha.Trim()
        if ($t.StartsWith("[") -or $t.StartsWith(";")) { continue }
        $i = $linha.IndexOf("=")
        if ($i -gt 0) { $v[$linha.Substring(0, $i).Trim().ToLower()] = $linha.Substring($i + 1) }
    }
    return $v
}

function Procurar-Conexao {
    $candidatos = New-Object System.Collections.Generic.List[string]
    if ($PSScriptRoot) {
        $candidatos.Add((Join-Path $PSScriptRoot "Conexao.ini"))
        $acima = Split-Path -Parent $PSScriptRoot
        if ($acima) { $candidatos.Add((Join-Path $acima "Conexao.ini")) }
    }
    $raizes = @("C:\", "D:\", "E:\", $env:ProgramFiles, ${env:ProgramFiles(x86)}, $env:USERPROFILE, (Join-Path $env:USERPROFILE "Desktop"))
    foreach ($raiz in $raizes) {
        if (-not $raiz -or -not (Test-Path -LiteralPath $raiz)) { continue }
        foreach ($d in @(Get-ChildItem -LiteralPath $raiz -Directory -ErrorAction SilentlyContinue | Where-Object { $_.Name -like "*kigest*" })) {
            $candidatos.Add((Join-Path $d.FullName "Conexao.ini"))
            foreach ($s in @(Get-ChildItem -LiteralPath $d.FullName -Directory -ErrorAction SilentlyContinue)) {
                $candidatos.Add((Join-Path $s.FullName "Conexao.ini"))
            }
        }
    }
    $existentes = @($candidatos | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -Unique)
    # o do GC primeiro (o POS costuma apontar para «localhost»)
    $gc = @($existentes | Where-Object { $_ -match "GC" })
    return @($gc + ($existentes | Where-Object { $gc -notcontains $_ }))
}

function Escolher-Conexao {
    try {
        Add-Type -AssemblyName System.Windows.Forms
        $d = New-Object System.Windows.Forms.OpenFileDialog
        $d.Title = "Escolha o Conexao.ini do KIGEST"
        $d.Filter = "Conexao.ini|Conexao.ini|Ficheiros INI (*.ini)|*.ini"
        if ($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { return $d.FileName }
    } catch { }
    return $null
}

function Testar-Ligacao([string]$servidor, [string]$base, [string]$utilizador, [string]$senha, [bool]$windows) {
    $b = New-Object System.Data.SqlClient.SqlConnectionStringBuilder
    $b["Data Source"] = $servidor
    $b["Initial Catalog"] = $base
    if ($windows) {
        $b["Integrated Security"] = $true
    } else {
        $b["User ID"] = $utilizador
        $b["Password"] = $senha
    }
    $b["Connect Timeout"] = 10
    $b["Application Name"] = "KanGest - exportacao"
    $cn = New-Object System.Data.SqlClient.SqlConnection $b.ConnectionString
    try {
        $cn.Open()
        return $b.ConnectionString
    } catch {
        $e = $_.Exception
        while ($e -and -not ($e -is [System.Data.SqlClient.SqlException])) { $e = $e.InnerException }
        if ($e) {
            $script:UltimoErro = $e.Message
            # 18456 = login falhado: não insistir com a mesma senha noutros nomes do servidor
            if ($e.Number -eq 18456 -and -not $windows) { $script:LoginFalhou = $true }
        } else {
            $script:UltimoErro = $_.Exception.Message
        }
        return $null
    } finally {
        $cn.Dispose()
    }
}

Write-Host ""
Write-Host "  KanGest - Exportar dados do KIGEST" -ForegroundColor Cyan
Write-Host "  ---------------------------------" -ForegroundColor Cyan
Write-Host ""

# 1) Conexao.ini
$inis = @()
if ($Conexao) { $inis = @($Conexao) } else { $inis = @(Procurar-Conexao) }
if ($inis.Count -eq 0) {
    Write-Host "  Não encontrei o Conexao.ini do KIGEST. Escolha-o na janela que vai abrir." -ForegroundColor Yellow
    $escolhido = Escolher-Conexao
    if ($escolhido) { $inis = @($escolhido) }
}
$base = "KigestGC"
$utilizador = "sa"
$senhas = @()
$servidores = New-Object System.Collections.Generic.List[string]
if ($Servidor) { $servidores.Add($Servidor) }
foreach ($ini in $inis) {
    try {
        $v = Ler-Ini $ini
        Write-Host "  Conexao.ini: $ini"
        if ($v["source"]) { $servidores.Add($v["source"].Trim()) }
        if ($v["bdados"]) { $base = $v["bdados"].Trim() }
        if ($v["user"]) { $utilizador = $v["user"].Trim() }
        if ($v.ContainsKey("senha")) { $senhas += $v["senha"].Trim(); $senhas += $v["senha"] }
    } catch {
        Write-Host "  Não foi possível ler $ini" -ForegroundColor Yellow
    }
}
foreach ($s in @("localhost", "(local)", ".\SQLEXPRESS", "localhost\SQLEXPRESS", $env:COMPUTERNAME, "$env:COMPUTERNAME\SQLEXPRESS")) { $servidores.Add($s) }
$servidores = @($servidores | Where-Object { $_ } | Select-Object -Unique)
$senhas = @($senhas | Select-Object -Unique)

# 2) Ligar ao SQL Server
Write-Host ""
Write-Host "  A procurar a base $base..."
$ligacao = $null
$usado = ""
$script:LoginFalhou = $false
$script:UltimoErro = ""
while (-not $ligacao) {
    foreach ($srv in $servidores) {
        foreach ($s in $senhas) {
            if ($script:LoginFalhou) { break }
            $ligacao = Testar-Ligacao $srv $base $utilizador $s $false
            if ($ligacao) { break }
        }
        if (-not $ligacao) { $ligacao = Testar-Ligacao $srv $base "" "" $true }
        if ($ligacao) { $usado = $srv; break }
    }
    if ($ligacao) { break }
    Write-Host ""
    Write-Host "  Não foi possível ligar à base $base." -ForegroundColor Red
    Write-Host "  Último erro: $script:UltimoErro" -ForegroundColor DarkGray
    Write-Host "  Corra este programa no computador onde está o SQL Server do KIGEST," -ForegroundColor Yellow
    Write-Host "  ou escreva o nome do servidor (ex.: DESKTOP-G26LDJM ou SERVIDOR\SQLEXPRESS)." -ForegroundColor Yellow
    $novo = Read-Host "  Servidor (Enter para sair)"
    if (-not $novo) { Fim 1 }
    $servidores = @($novo.Trim())
    $script:LoginFalhou = $false
}
Write-Host "  Ligado a $usado / $base" -ForegroundColor Green

# 3) Exportar
try {
    Add-Type -TypeDefinition $Codigo -ReferencedAssemblies System.Data, System.Xml, System.Transactions
} catch {
    Write-Host "  Não foi possível preparar o exportador: $($_.Exception.Message)" -ForegroundColor Red
    Fim 1
}
if (-not $Destino) { $Destino = [Environment]::GetFolderPath("Desktop") }
if (-not (Test-Path -LiteralPath $Destino)) { New-Item -ItemType Directory -Path $Destino | Out-Null }
$nome = "KIGEST-exportacao-" + (Get-Date -Format "yyyyMMdd-HHmm") + ".kigest"
$final = Join-Path $Destino $nome
$parcial = $final + ".parcial"
$origem = "$usado/$base"

Write-Host ""
Write-Host "  A exportar (só leitura, nada é alterado no KIGEST):"
$inicio = Get-Date
try {
    $r = [KigestExportador]::Exportar($ligacao, $origem, $parcial, [string[]]$Vistas)
} catch {
    Write-Host ""
    Write-Host "  A exportação falhou: $($_.Exception.Message)" -ForegroundColor Red
    Remove-Item -LiteralPath $parcial -ErrorAction SilentlyContinue
    Fim 1
}
Move-Item -LiteralPath $parcial -Destination $final -Force
$mb = [Math]::Round((Get-Item -LiteralPath $final).Length / 1MB, 1)
$seg = [Math]::Round(((Get-Date) - $inicio).TotalSeconds)

Write-Host ""
Write-Host "  Exportação concluída: $($r[0]) tabelas, $($r[1]) registos, $mb MB, $seg s" -ForegroundColor Green
if ($r.Count -gt 2) {
    Write-Host "  Avisos:" -ForegroundColor Yellow
    for ($i = 2; $i -lt $r.Count; $i++) { Write-Host "   - $($r[$i])" -ForegroundColor Yellow }
}
Write-Host ""
Write-Host "  Ficheiro: $final" -ForegroundColor Cyan
Write-Host ""
Write-Host "  Leve este ficheiro (pen, rede ou email) para o computador do KanGest e abra"
Write-Host "  Sistema -> Importar dados do KIGEST -> Ficheiro exportado."
Write-Host "  O ficheiro contém os dados da empresa: guarde-o em local seguro e apague-o depois da importação."
try { Start-Process explorer.exe -ArgumentList "/select,`"$final`"" } catch { }
Fim 0
