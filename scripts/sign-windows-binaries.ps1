# ==============================================================================
# ClearCore - Utilitario de Assinatura Digital de Teste (Windows PowerShell)
# ==============================================================================
# Este script cria um certificado de assinatura de codigo autoassinado (Code Signing)
# e assina os binarios do ClearCore (.exe e .sys) para testes em ambiente Windows.
#
# Uso:
#   .\sign-windows-binaries.ps1                    # Assina todos os binarios encontrados
#   .\sign-windows-binaries.ps1 -CreateCertificate # Apenas cria/exporta o certificado
#   .\sign-windows-binaries.ps1 -InstallToRoot     # Confia no certificado nesta maquina
#   .\sign-windows-binaries.ps1 -Target "caminho"  # Assina um arquivo especifico
# ==============================================================================

[CmdletBinding()]
param(
    [switch]$CreateCertificate,
    [switch]$InstallToRoot,
    [string]$Target
)

$ErrorActionPreference = "Stop"

$CertSubject = "CN=ClearCore Open Source Test Certificate"
$CertFriendlyName = "ClearCore Test Code Signing Certificate"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$OutputDir = $ScriptDir

# 1. Obter ou Criar Certificado de Assinatura de Codigo
function Get-OrCreateCodeSigningCert {
    Write-Host "[1/3] Verificando certificado de assinatura de codigo..." -ForegroundColor Cyan

    $cert = Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert -ErrorAction SilentlyContinue | 
            Where-Object { $_.Subject -eq $CertSubject } | 
            Select-Object -First 1

    if (-not $cert) {
        Write-Host "      Criando novo certificado autoassinado para testes..." -ForegroundColor Yellow
        $cert = New-SelfSignedCertificate `
            -Type CodeSigningCert `
            -Subject $CertSubject `
            -KeyUsage DigitalSignature `
            -FriendlyName $CertFriendlyName `
            -CertStoreLocation Cert:\CurrentUser\My `
            -NotAfter (Get-Date).AddYears(3)
        Write-Host "      [OK] Certificado criado com sucesso (Thumbprint: $($cert.Thumbprint))" -ForegroundColor Green
    } else {
        Write-Host "      [OK] Certificado existente encontrado (Thumbprint: $($cert.Thumbprint))" -ForegroundColor Green
    }

    # Exportar certificado publico (.cer)
    $cerPath = Join-Path $OutputDir "ClearCoreTestCertificate.cer"
    Export-Certificate -Cert $cert -FilePath $cerPath -Force | Out-Null
    Write-Host "      [OK] Certificado publico exportado para: $cerPath" -ForegroundColor Green

    return $cert
}

# 2. Instalar Certificado nas Autoridades Raiz Confiaveis (Opcional para testes locais)
function Install-CertToRoot {
    param([System.Security.Cryptography.X509Certificates.X509Certificate2]$Certificate)

    Write-Host "[2/3] Instalando certificado nas Autoridades Raiz Confiaveis..." -ForegroundColor Cyan
    try {
        $store = New-Object System.Security.Cryptography.X509Certificates.X509Store "Root", "CurrentUser"
        $store.Open("ReadWrite")
        $store.Add($Certificate)
        $store.Close()
        Write-Host "      [OK] Certificado adicionado as Autoridades Raiz Confiaveis do usuario." -ForegroundColor Green
        Write-Host "           O Windows agora reconhecera os binarios assinados como confiaveis localmente." -ForegroundColor Gray
    } catch {
        Write-Warning "Nao foi possivel instalar automaticamente na raiz: $_"
        Write-Host "      Voce pode instalar manualmente clicando duas vezes em ClearCoreTestCertificate.cer" -ForegroundColor Gray
    }
}

# 3. Assinar Arquivo Binario
function Sign-BinaryFile {
    param(
        [string]$FilePath,
        [System.Security.Cryptography.X509Certificates.X509Certificate2]$Certificate
    )

    if (-not (Test-Path $FilePath)) {
        Write-Warning "Arquivo nao encontrado para assinatura: $FilePath"
        return
    }

    Write-Host "      Assinando: $FilePath ..." -NoNewline
    try {
        $sig = Set-AuthenticodeSignature -FilePath $FilePath -Certificate $Certificate -HashAlgorithm SHA256 -TimestampServer "http://timestamp.digicert.com" -ErrorAction SilentlyContinue
        if ($sig.Status -eq "Valid") {
            Write-Host " [VALIDO]" -ForegroundColor Green
        } else {
            # Se o timestamp server falhar ou estiver offline, assina sem timestamp
            $sig = Set-AuthenticodeSignature -FilePath $FilePath -Certificate $Certificate -HashAlgorithm SHA256
            Write-Host " [ASSINADO (Sem timestamp: $($sig.Status))]" -ForegroundColor Yellow
        }
    } catch {
        Write-Host " [FALHA: $_]" -ForegroundColor Red
    }
}

# Fluxo Principal
$signingCert = Get-OrCreateCodeSigningCert

if ($InstallToRoot) {
    Install-CertToRoot -Certificate $signingCert
}

if ($CreateCertificate -and (-not $Target)) {
    Write-Host ""
    Write-Host "Certificado pronto para uso!" -ForegroundColor Green
    Write-Host "Para assinar binarios individuais, use: .\sign-windows-binaries.ps1 -Target <arquivo.exe>"
    Write-Host "Para confiar no certificado nesta maquina: .\sign-windows-binaries.ps1 -InstallToRoot"
    exit 0
}

Write-Host "[3/3] Assinando binarios do ClearCore..." -ForegroundColor Cyan

if ($Target) {
    Sign-BinaryFile -FilePath $Target -Certificate $signingCert
    exit 0
}

# Procurar binarios do ClearCore em locais conhecidos
$SearchTargets = @(
    (Join-Path $ScriptDir "..\..\Clearcore.exe"),
    (Join-Path $ScriptDir "..\Clearcore.exe"),
    (Join-Path $ScriptDir "Clearcore.exe"),
    (Join-Path $ScriptDir "..\release\Clearcore-win32-x64\Clearcore.exe"),
    (Join-Path $ScriptDir "..\release\Clearcore-win32-x64\resources\bin\realtime-noise-service.exe"),
    (Join-Path $ScriptDir "..\target\release\realtime-noise-service.exe"),
    (Join-Path $ScriptDir "..\target\release\realtime-noise-app-tauri.exe"),
    (Join-Path $ScriptDir "..\platform\windows\driver\RealtimeNoise.sys"),
    (Join-Path $ScriptDir "..\release\Clearcore-win32-x64\resources\driver\RealtimeNoise.sys")
)

$signedCount = 0
foreach ($t in $SearchTargets) {
    if (Test-Path $t) {
        $resolved = (Resolve-Path $t).Path
        Sign-BinaryFile -FilePath $resolved -Certificate $signingCert
        $signedCount++
    }
}

Write-Host ""
if ($signedCount -gt 0) {
    Write-Host "[CONCLUIDO] $signedCount binario(s) processado(s) com sucesso!" -ForegroundColor Green
} else {
    Write-Host "[INFO] Nenhum binario encontrado nos caminhos padrao." -ForegroundColor Yellow
    Write-Host "       Especifique o arquivo com: .\sign-windows-binaries.ps1 -Target 'caminho\arquivo.exe'" -ForegroundColor Gray
}

Write-Host ""
Write-Host "=== Informacao Importante para Drivers de Kernel (WaveRT) ===" -ForegroundColor Yellow
Write-Host "Para drivers de kernel (.sys) no Windows 10/11:"
Write-Host "1. O Windows exige que o Modo de Testes (Test Signing) esteja ativado:"
Write-Host "   bcdedit /set testsigning on  (requer prompt como Administrador)"
Write-Host "2. Reinicie o computador se for a primeira vez ativando o modo de testes."
Write-Host "3. O driver assinado com este certificado sera aceito normalmente pelo Windows."
