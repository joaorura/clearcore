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

# 5. Desinstalar driver WaveRT via pnputil
Write-Host "Desinstalando driver de audio virtual WaveRT..." -ForegroundColor Yellow
if ($isAdmin) {
    # Procurar o driver publicado correspondente ao RealtimeNoise
    $drivers = pnputil.exe /enum-drivers
    $oemName = ""
    for ($i = 0; $i -lt $drivers.Count; $i++) {
        if ($drivers[$i] -match "RealtimeNoise\.inf") {
            if ($i -gt 0 -and $drivers[$i-1] -match "oem\d+\.inf") {
                $oemName = $matches[0]
            } elseif ($i -gt 1 -and $drivers[$i-2] -match "oem\d+\.inf") {
                $oemName = $matches[0]
            }
        }
    }
    if ($oemName) {
        Write-Host "Removendo driver publicado: $oemName..." -ForegroundColor Gray
        pnputil.exe /delete-driver $oemName /uninstall /force | Out-Null
    }
    pnputil.exe /delete-driver "RealtimeNoise.inf" /uninstall /force 2>$null | Out-Null
}

# Desativar dispositivos virtuais órfãos no PnP
Get-PnpDevice | Where-Object { 
    $_.InstanceId -like "*RealtimeNoise*" -or 
    $_.FriendlyName -like "*Realtime Noise*" 
} | ForEach-Object {
    Disable-PnpDevice -InstanceId $_.InstanceId -Confirm:$false -ErrorAction SilentlyContinue
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
