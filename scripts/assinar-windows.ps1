# Assina um ficheiro do Windows com o jsign (chamado pelo Tauri: bundle.windows.signCommand, ver build.yml).
# Variáveis (segredos do GitHub):
#   WINDOWS_ASSINATURA_TIPO         serviço do jsign; por omissão ESIGNER (SSL.com eSigner)
#   WINDOWS_ASSINATURA_CONTA        ESIGNER: «utilizador|palavra-passe» da conta SSL.com
#   WINDOWS_ASSINATURA_SEGREDO      ESIGNER: segredo TOTP (base64) do eSigner
#   WINDOWS_ASSINATURA_CERTIFICADO  ESIGNER: identificador do certificado (credential ID)
#   WINDOWS_ASSINATURA_COFRE        opcional: --keystore (outros serviços)
#   WINDOWS_ASSINATURA_TSA          opcional: servidor de carimbo de tempo
param([Parameter(Mandatory = $true)][string]$Ficheiro)
$ErrorActionPreference = "Stop"

$tipo = if ($env:WINDOWS_ASSINATURA_TIPO) { $env:WINDOWS_ASSINATURA_TIPO } else { "ESIGNER" }
$tsa = if ($env:WINDOWS_ASSINATURA_TSA) { $env:WINDOWS_ASSINATURA_TSA } elseif ($tipo -eq "ESIGNER") { "http://ts.ssl.com" } else { "http://timestamp.digicert.com" }
$parametros = @(
  "-jar", $env:JSIGN_JAR,
  "--storetype", $tipo,
  "--storepass", $env:WINDOWS_ASSINATURA_CONTA,
  "--alg", "SHA-256",
  "--tsaurl", $tsa, "--tsmode", "RFC3161", "--tsretries", "3",
  "--name", "KanGest", "--url", "https://kangest.pages.dev",
  "--replace"
)
if ($env:WINDOWS_ASSINATURA_SEGREDO) { $parametros += @("--keypass", $env:WINDOWS_ASSINATURA_SEGREDO) }
if ($env:WINDOWS_ASSINATURA_CERTIFICADO) { $parametros += @("--alias", $env:WINDOWS_ASSINATURA_CERTIFICADO) }
if ($env:WINDOWS_ASSINATURA_COFRE) { $parametros += @("--keystore", $env:WINDOWS_ASSINATURA_COFRE) }

Write-Host "A assinar $(Split-Path -Leaf $Ficheiro)…"
& java @args $Ficheiro
if ($LASTEXITCODE -ne 0) { throw "A assinatura de $Ficheiro falhou (jsign $LASTEXITCODE)." }
