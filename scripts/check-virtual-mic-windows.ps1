# ==============================================================================
# Clearcore / Hippocamp - Verificador e Instalador de Microfone Virtual (Windows)
# ==============================================================================
# Este script:
# 1. Checa se o driver e dispositivo WaveRT "Root\RealtimeNoise" existe no Windows.
# 2. Se NÃO foi criado, inicia o processo de registro e instalação do driver INF.
# 3. Garante que o microfone esteja visível no Windows CoreAudio / MMDevice.
#
# Uso:
#   .\check-virtual-mic-windows.ps1              # Verifica e cria/instala se não existir
#   .\check-virtual-mic-windows.ps1 -CheckOnly   # Apenas checa existência (exit 0=existe, 1=não)
#   .\check-virtual-mic-windows.ps1 -Json        # Retorna status em formato JSON
#   .\check-virtual-mic-windows.ps1 -Recreate    # Força reinstalação/recriação do driver
#   .\check-virtual-mic-windows.ps1 -SetDefault  # Define como microfone padrão no Windows
#   .\check-virtual-mic-windows.ps1 -Status      # Exibe status formatado
# ==============================================================================

[CmdletBinding()]
param(
    [switch]$CheckOnly,
    [switch]$Json,
    [switch]$Recreate,
    [switch]$SetDefault,
    [switch]$Status
)

$ErrorActionPreference = "SilentlyContinue"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path "$ScriptDir\.."
$InfPath = "$RepoRoot\platform\windows\driver\RealtimeNoise.inf"
$DeviceHardwareId = "Root\RealtimeNoise"
$DeviceName = "\\.\RealtimeNoise"
$DeviceDesc = "Realtime Noise Virtual Audio Capture Device"

function Test-VirtualDevicePresent {
    # 1. Checar via PnP Device
    $pnp = Get-PnpDevice | Where-Object { 
        $_.InstanceId -like "*RealtimeNoise*" -or 
        $_.FriendlyName -like "*Realtime Noise*" -or 
        $_.FriendlyName -like "*Hippocamp*" 
    }
    if ($pnp -and ($pnp.Status -eq "OK" -or $pnp.Present -eq $true)) {
        return $true
    }

    # 2. Checar via Service
    $svc = Get-Service -Name "RealtimeNoise" -ErrorAction SilentlyContinue
    if ($svc -and $svc.Status -eq "Running") {
        return $true
    }

    # 3. Checar device path
    if (Test-Path $DeviceName) {
        return $true
    }

    return $false
}

function Get-VirtualDeviceStatus {
    $pnp = Get-PnpDevice | Where-Object { 
        $_.InstanceId -like "*RealtimeNoise*" -or 
        $_.FriendlyName -like "*Realtime Noise*" -or 
        $_.FriendlyName -like "*Hippocamp*" 
    } | Select-Object -First 1

    if ($pnp) {
        return $pnp.Status
    }
    return "NotPresent"
}

function Test-IsDefaultMic {
    # Testa se é o dispositivo de gravação padrão no Windows
    if (Get-Command Get-AudioDevice -ErrorAction SilentlyContinue) {
        $def = Get-AudioDevice -Recording -ErrorAction SilentlyContinue
        if ($def -and ($def.Name -like "*Realtime Noise*" -or $def.Name -like "*Hippocamp*")) {
            return $true
        }
    }
    return $false
}

function Set-DefaultMic {
    if (Get-Command Set-AudioDevice -ErrorAction SilentlyContinue) {
        $dev = Get-AudioDevice -List -ErrorAction SilentlyContinue | Where-Object { 
            $_.Type -eq "Recording" -and ($_.Name -like "*Realtime Noise*" -or $_.Name -like "*Hippocamp*") 
        } | Select-Object -First 1
        if ($dev) {
            Set-AudioDevice -Index $dev.Index | Out-Null
            return $true
        }
    }
    return $false
}

function Install-VirtualDriver {
    Write-Host "[Windows] Instalando driver WaveRT do microfone virtual..."
    if (Test-Path $InfPath) {
        # 1. Tenta instalar diretamente via pnputil
        $pnpOut = & pnputil.exe /add-driver "$InfPath" /install 2>&1
        if (Test-VirtualDevicePresent) {
            return $true
        }

        # 2. Se falhar, solicita elevação UAC para instalar o driver
        Write-Host "[Windows] Solicitando elevacao de Administrador (UAC) para instalar o driver..."
        try {
            $argList = "/c pnputil.exe /add-driver `"$InfPath`" /install"
            $proc = Start-Process -FilePath "cmd.exe" -ArgumentList $argList -Verb RunAs -Wait -PassThru -WindowStyle Hidden
        } catch {
            Write-Warning "Falha na solicitacao de elevacao UAC: $_"
        }

        # 3. Tenta devcon se disponível para instanciar Root\RealtimeNoise
        if (Get-Command devcon.exe -ErrorAction SilentlyContinue) {
            devcon.exe install "$InfPath" "$DeviceHardwareId" 2>&1 | Out-Null
        }

        # 4. Iniciar o serviço caso tenha sido registrado
        Start-Service -Name "RealtimeNoise" -ErrorAction SilentlyContinue

        Start-Sleep -Seconds 1
        return (Test-VirtualDevicePresent)
    } else {
        Write-Warning "Arquivo INF nao encontrado em: $InfPath"
        return $false
    }
}

function Output-JsonStatus {
    $isPresent = Test-VirtualDevicePresent
    $driverStatus = Get-VirtualDeviceStatus
    $isDef = Test-IsDefaultMic

    $obj = [PSCustomObject]@{
        platform         = "windows"
        platform_label   = "Windows (PortCls / WaveRT Driver)"
        present          = $isPresent
        node_id          = if ($isPresent) { $DeviceHardwareId } else { $null }
        node_name        = $DeviceName
        node_description = $DeviceDesc
        driver_status    = $driverStatus
        is_default       = $isDef
        format           = "F32LE / PCM16"
        rate             = 48000
        channels         = 1
        quantum          = 480
        install_inf      = $InfPath
    }
    $obj | ConvertTo-Json -Compress
}

# Despacho de comandos
if ($CheckOnly) {
    if (Test-VirtualDevicePresent) {
        if ($Json) { Output-JsonStatus }
        exit 0
    } else {
        if ($Json) { Output-JsonStatus }
        exit 1
    }
}

if ($SetDefault) {
    $success = Set-DefaultMic
    if ($Json) {
        Output-JsonStatus
    } else {
        if ($success) {
            Write-Host "[OK] Microfone virtual definido como padrao no Windows!"
        } else {
            Write-Warning "Nao foi possivel alterar o dispositivo padrao automaticamente. Selecione nas Configuracoes de Som do Windows."
        }
    }
    exit 0
}

if ($Recreate) {
    $success = Install-VirtualDriver
    if ($Json) {
        Output-JsonStatus
    } else {
        if ($success) {
            Write-Host "[SUCESSO] Microfone virtual criado e registrado no Windows!"
        } else {
            Write-Error "[ERRO] Falha ao instalar o driver do microfone virtual no Windows. Execute como Administrador: pnputil /add-driver '$InfPath' /install"
        }
    }
    if ($success) { exit 0 } else { exit 1 }
}

if ($Status) {
    if ($Json) {
        Output-JsonStatus
        exit 0
    }
    $present = Test-VirtualDevicePresent
    $driverStat = Get-VirtualDeviceStatus
    $isDef = if (Test-IsDefaultMic) { "Sim (Dispositivo Primario)" } else { "Nao" }

    Write-Host "=========================================================="
    Write-Host " Microfone Virtual Clearcore (Windows WaveRT)"
    Write-Host "=========================================================="
    if ($present) {
        Write-Host " - Estado:       [OK] Ativo no Sistema"
        Write-Host " - Hardware ID:  $DeviceHardwareId"
        Write-Host " - Device Path:  $DeviceName"
        Write-Host " - Descricao:    $DeviceDesc"
        Write-Host " - Status PnP:   $driverStat"
        Write-Host " - Padrao:       $isDef"
        Write-Host " - Formato:      48kHz Float32 / PCM16 Mono (10ms)"
    } else {
        Write-Host " - Estado:       [AUSENTE] Driver ou Dispositivo nao detectado"
        Write-Host " - Instalacao:   Execute como Administrador:"
        Write-Host "                 pnputil /add-driver `"$InfPath`" /install"
    }
    Write-Host "=========================================================="
    if ($present) { exit 0 } else { exit 1 }
}

# Acao padrao: checar e criar/instalar se nao existir
if (Test-VirtualDevicePresent) {
    if ($Json) {
        Output-JsonStatus
    } else {
        Write-Host "[OK] Microfone virtual WaveRT verificado e ativo no Windows! ($DeviceHardwareId)"
    }
    exit 0
} else {
    if (-not $Json) {
        Write-Host "[AVISO] Microfone virtual WaveRT nao detectado no Windows! Tentando criar/instalar..."
    }
    $created = Install-VirtualDriver
    if ($Json) {
        Output-JsonStatus
    } else {
        if ($created) {
            Write-Host "[SUCESSO] Microfone virtual instalado com sucesso no Windows!"
        } else {
            Write-Warning "Requer elevacao de Administrador para instalar o driver. Execute: pnputil /add-driver `"$InfPath`" /install"
        }
    }
    if ($created) { exit 0 } else { exit 1 }
}
