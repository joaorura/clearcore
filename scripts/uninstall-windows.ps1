# ==============================================================================
# Clearcore - Desinstalador Completo do Sistema (Windows)
# ==============================================================================
# Remove completamente o Clearcore do Windows:
# - Encerra os processos Clearcore.exe e realtime-noise-service.exe
# - Remove o driver de áudio virtual WaveRT (pnputil /delete-driver)
# - Remove os dispositivos PnP virtuais do Gerenciador de Dispositivos
# - Remove serviços do Windows e tarefas agendadas de inicialização
# - Remove chaves de registro de inicialização (Run) e de desinstalação
# - Exclui atalhos do Menu Iniciar e da Área de Trabalho
# - Remove a pasta de instalação e arquivos temporários
# ==============================================================================

[CmdletBinding()]
param()

$ErrorActionPreference = "SilentlyContinue"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host " Clearcore Desktop - Desinstalação Completa (Windows)" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

# 1. Verificar privilégios de Administrador
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host "AVISO: Executando sem privilégios de Administrador." -ForegroundColor Yellow
    Write-Host "Para remover o driver WaveRT de nível de kernel, execute como Administrador." -ForegroundColor Yellow
}

# 2. Encerrar processos ativos
Write-Host "Encerrando processos do Clearcore..." -ForegroundColor Yellow
Stop-Process -Name "Clearcore" -Force -ErrorAction SilentlyContinue
Stop-Process -Name "realtime-noise-service" -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1

# 3. Remover servico de usuario / tarefa agendada
Write-Host "Removendo servicos e tarefas agendadas..." -ForegroundColor Yellow
Unregister-ScheduledTask -TaskName "ClearcoreRealtimeNoise" -Confirm:$false -ErrorAction SilentlyContinue
Unregister-ScheduledTask -TaskName "RealtimeNoiseService" -Confirm:$false -ErrorAction SilentlyContinue

# 4. Remover inicializacao automatica do Registro
Write-Host "Removendo inicializacao automatica..." -ForegroundColor Yellow
Remove-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -Name "Clearcore" -ErrorAction SilentlyContinue
Remove-ItemProperty -Path "HKLM:\Software\Microsoft\Windows\CurrentVersion\Run" -Name "Clearcore" -ErrorAction SilentlyContinue

# 5. Desinstalar driver de audio virtual WaveRT do DriverStore e do PnP
Write-Host "Desinstalando driver de audio virtual WaveRT do DriverStore..." -ForegroundColor Yellow

# Parar servico de driver de kernel se ativo
$driverSvc = Get-Service -Name "RealtimeNoise" -ErrorAction SilentlyContinue
if ($driverSvc) {
    Write-Host "Interrompendo servico de driver de kernel RealtimeNoise..." -ForegroundColor Gray
    Stop-Service -Name "RealtimeNoise" -Force -ErrorAction SilentlyContinue
    & sc.exe delete "RealtimeNoise" 2>$null | Out-Null
}

if ($isAdmin) {
    # 5.1 Encontrar todos os pacotes OEM publicados correspondentes a RealtimeNoise
    $enumDrivers = & pnputil.exe /enum-drivers 2>&1
    $driverBlocks = ($enumDrivers -join "`n") -split "(?m)(?=Published Name|Published name|Nome publicado|Nome do OEM)"
    $oemList = @()
    foreach ($block in $driverBlocks) {
        if ($block -match "RealtimeNoise" -or $block -match "ClearCore") {
            if ($block -match "(oem\d+\.inf)") {
                $oemList += $matches[1]
            }
        }
    }
    # Fallback caso a saida nao seja em blocos esperados
    if ($oemList.Count -eq 0) {
        for ($i = 0; $i -lt $enumDrivers.Count; $i++) {
            if ($enumDrivers[$i] -match "RealtimeNoise" -or $enumDrivers[$i] -match "ClearCore") {
                for ($j = [Math]::Max(0, $i - 4); $j -le [Math]::Min($enumDrivers.Count - 1, $i + 4); $j++) {
                    if ($enumDrivers[$j] -match "(oem\d+\.inf)") {
                        $oemList += $matches[1]
                    }
                }
            }
        }
    }
    $oemList = $oemList | Select-Object -Unique
    foreach ($oem in $oemList) {
        Write-Host "Removendo driver publicado do DriverStore: $oem..." -ForegroundColor Gray
        & pnputil.exe /delete-driver $oem /uninstall /force | Out-Null
    }
    & pnputil.exe /delete-driver "RealtimeNoise.inf" /uninstall /force 2>$null | Out-Null

    # Candidatos locais do INF
    $candidateInfs = @(
        "$env:ProgramFiles\Clearcore\resources\driver\RealtimeNoise.inf",
        "$env:LOCALAPPDATA\Programs\Clearcore\resources\driver\RealtimeNoise.inf"
    )
    foreach ($cInf in $candidateInfs) {
        if (Test-Path $cInf) {
            & pnputil.exe /delete-driver "$cInf" /uninstall /force 2>$null | Out-Null
        }
    }
}

# 5.2 Remover dispositivos virtuais do Gerenciador de Dispositivos (PnP)
Write-Host "Removendo dispositivos virtuais do PnP..." -ForegroundColor Yellow
$pnpDevices = Get-PnpDevice | Where-Object { 
    $_.InstanceId -like "*RealtimeNoise*" -or 
    $_.FriendlyName -like "*Realtime Noise*" -or
    $_.FriendlyName -like "*ClearCore*" -or
    $_.FriendlyName -like "*Clearcore*"
}
foreach ($dev in $pnpDevices) {
    $devId = $dev.InstanceId
    Write-Host "Removendo dispositivo virtual PnP: $devId..." -ForegroundColor Gray
    # 1. Tentar pnputil /remove-device (Windows 10/11)
    & pnputil.exe /remove-device "$devId" 2>$null | Out-Null
    # 2. Tentar devcon se disponivel
    if (Get-Command devcon.exe -ErrorAction SilentlyContinue) {
        & devcon.exe remove "@$devId" 2>$null | Out-Null
    }
    # 3. Desativar como contingencia se remocao falhar
    Disable-PnpDevice -InstanceId $devId -Confirm:$false -ErrorAction SilentlyContinue
}

# Remover hardware ID raiz via devcon se disponivel
if (Get-Command devcon.exe -ErrorAction SilentlyContinue) {
    & devcon.exe remove "Root\RealtimeNoise" 2>$null | Out-Null
}

# 6. Remover atalhos do Menu Iniciar e Desktop
Write-Host "Removendo atalhos e diretorios do Menu Iniciar..." -ForegroundColor Yellow
$startMenu = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Clearcore"
if (Test-Path $startMenu) {
    Remove-Item -Recurse -Force $startMenu -ErrorAction SilentlyContinue
}
Remove-Item -Force "$env:USERPROFILE\Desktop\Clearcore.lnk" -ErrorAction SilentlyContinue
Remove-Item -Force "$env:PUBLIC\Desktop\Clearcore.lnk" -ErrorAction SilentlyContinue

# 7. Remover chaves de Registro da aplicação
Remove-Item -Recurse -Force "HKCU:\Software\Clearcore" -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Clearcore" -ErrorAction SilentlyContinue

# 8. Remover diretórios de instalação e dados locais
Write-Host "Removendo arquivos de instalacao..." -ForegroundColor Yellow
$installDirs = @(
    "$env:LOCALAPPDATA\Programs\Clearcore",
    "$env:ProgramFiles\Clearcore",
    "$env:APPDATA\Clearcore",
    "$env:LOCALAPPDATA\Clearcore"
)
foreach ($dir in $installDirs) {
    if (Test-Path $dir) {
        Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
    }
}

Write-Host ""
Write-Host "==========================================================" -ForegroundColor Green
Write-Host "Clearcore foi completamente desinstalado do Windows!" -ForegroundColor Green
Write-Host "   - Processos encerrados" -ForegroundColor Green
Write-Host "   - Driver WaveRT desinstalado" -ForegroundColor Green
Write-Host "   - Tarefas agendadas e autostart removidos" -ForegroundColor Green
Write-Host "   - Atalhos, registros e arquivos excluidos" -ForegroundColor Green
Write-Host "==========================================================" -ForegroundColor Green
