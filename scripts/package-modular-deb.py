#!/usr/bin/env python3
"""
Modular Debian package generator for Clearcore.
Splits the bundle into:
  1. clearcore-daemon (~15 MB): rust daemon, DeepFilterNet3 ONNX, pipewire helper/C bridge
  2. clearcore (~85 MB): Electron GUI and system tray (Depends: clearcore-daemon)
Both packages are strictly under the 100 MB GitHub file size limit.
"""
import os
import sys
import shutil
import tarfile
import io
import gzip

def build_deb(out_deb, control_dict, data_root, scripts_dict=None):
    # 1. debian-binary
    deb_bin = b"2.0\n"

    # 2. control.tar.gz
    ctrl_lines = []
    for k, v in control_dict.items():
        ctrl_lines.append(f"{k}: {v}\n")
    ctrl_content = "".join(ctrl_lines).encode("utf-8")

    ctrl_buf = io.BytesIO()
    with tarfile.open(fileobj=ctrl_buf, mode="w:gz") as tar:
        ti = tarfile.TarInfo("./control")
        ti.size = len(ctrl_content)
        ti.mode = 0o644
        tar.addfile(ti, io.BytesIO(ctrl_content))

        if scripts_dict:
            for sname, sbody in scripts_dict.items():
                sbytes = sbody.encode("utf-8") if isinstance(sbody, str) else sbody
                sti = tarfile.TarInfo(f"./{sname}")
                sti.size = len(sbytes)
                sti.mode = 0o755
                tar.addfile(sti, io.BytesIO(sbytes))
    ctrl_bytes = ctrl_buf.getvalue()

    # 3. data.tar.gz
    data_buf = io.BytesIO()
    with tarfile.open(fileobj=data_buf, mode="w:gz") as tar:
        for root, dirs, files in os.walk(data_root):
            for d in sorted(dirs):
                full_path = os.path.join(root, d)
                rel_path = "." + full_path[len(data_root):]
                ti = tar.gettarinfo(full_path, arcname=rel_path)
                ti.uid = 0
                ti.gid = 0
                ti.uname = "root"
                ti.gname = "root"
                tar.addfile(ti)
            for f in sorted(files):
                full_path = os.path.join(root, f)
                rel_path = "." + full_path[len(data_root):]
                ti = tar.gettarinfo(full_path, arcname=rel_path)
                ti.uid = 0
                ti.gid = 0
                ti.uname = "root"
                ti.gname = "root"
                if ti.isreg():
                    with open(full_path, "rb") as fp:
                        tar.addfile(ti, fp)
                else:
                    tar.addfile(ti)
    data_bytes = data_buf.getvalue()

    # 4. Assemble AR archive
    with open(out_deb, "wb") as f:
        f.write(b"!<arch>\n")
        backtick = chr(96)
        for name, content in [("debian-binary", deb_bin), ("control.tar.gz", ctrl_bytes), ("data.tar.gz", data_bytes)]:
            hdr = f"{name:<16}{0:<12}{0:<6}{0:<6}{0o100644:<8o}{len(content):<10}{backtick}\n".encode("ascii")
            f.write(hdr)
            f.write(content)
            if len(content) % 2 != 0:
                f.write(b"\n")

def make_modular_debs(src_dir, out_dir, version="0.1.0-beta.3", arch="amd64"):
    os.makedirs(out_dir, exist_ok=True)
    temp_work = os.path.join(out_dir, f"_temp_modular_{arch}")
    shutil.rmtree(temp_work, ignore_errors=True)

    daemon_root = os.path.join(temp_work, "clearcore-daemon")
    gui_root = os.path.join(temp_work, "clearcore")

    os.makedirs(os.path.join(daemon_root, "usr/bin"), exist_ok=True)
    os.makedirs(os.path.join(daemon_root, "usr/lib/clearcore"), exist_ok=True)
    os.makedirs(os.path.join(daemon_root, "usr/share/clearcore"), exist_ok=True)

    # 1. Daemon files
    daemon_bin = os.path.join(src_dir, "resources/bin/realtime-noise-service")
    helper_bin = os.path.join(src_dir, "resources/bin/pipewire_helper")
    filter_so = os.path.join(src_dir, "libclearcore_filter.so")
    if not os.path.exists(filter_so):
        filter_so = os.path.join(src_dir, "resources/bin/libclearcore_filter.so")

    if os.path.exists(daemon_bin):
        shutil.copy2(daemon_bin, os.path.join(daemon_root, "usr/bin/realtime-noise-service"))
        os.chmod(os.path.join(daemon_root, "usr/bin/realtime-noise-service"), 0o755)
    if os.path.exists(helper_bin):
        shutil.copy2(helper_bin, os.path.join(daemon_root, "usr/bin/clearcore-pipewire-helper"))
        os.chmod(os.path.join(daemon_root, "usr/bin/clearcore-pipewire-helper"), 0o755)
    if os.path.exists(filter_so):
        shutil.copy2(filter_so, os.path.join(daemon_root, "usr/lib/clearcore/libclearcore_filter.so"))
        os.chmod(os.path.join(daemon_root, "usr/lib/clearcore/libclearcore_filter.so"), 0o755)

    if os.path.exists(os.path.join(src_dir, "vendor")):
        shutil.copytree(os.path.join(src_dir, "vendor"), os.path.join(daemon_root, "usr/share/clearcore/vendor"))
    if os.path.exists(os.path.join(src_dir, "governance")):
        shutil.copytree(os.path.join(src_dir, "governance"), os.path.join(daemon_root, "usr/share/clearcore/governance"))
    if os.path.exists(os.path.join(src_dir, "resources/scripts")):
        shutil.copytree(os.path.join(src_dir, "resources/scripts"), os.path.join(daemon_root, "usr/share/clearcore/scripts"))

    daemon_control = {
        "Package": "clearcore-daemon",
        "Version": version,
        "Section": "sound",
        "Priority": "optional",
        "Architecture": arch,
        "Maintainer": "João Rura <joaorura@users.noreply.github.com>",
        "Depends": "pipewire (>= 0.3.0)",
        "Description": "Clearcore background noise suppression daemon and PipeWire bridge\n DeepFilterNet3 ONNX speech isolation, Neural EQ calibration, and studio DSP.",
    }
    daemon_scripts = {
        "prerm": """#!/bin/sh
set -e
case "$1" in
    remove|purge|deconfigure)
        killall -q realtime-noise-service 2>/dev/null || pkill -x realtime-noise-service 2>/dev/null || true
        killall -q clearcore-pipewire-helper 2>/dev/null || pkill -x clearcore-pipewire-helper 2>/dev/null || true
        killall -q pipewire_helper 2>/dev/null || pkill -x pipewire_helper 2>/dev/null || true
        pkill -f "^pw-loopback.*realtime-noise" 2>/dev/null || true

        if command -v pw-cli >/dev/null 2>&1; then
            for nid in $(pw-cli list-objects Node 2>/dev/null | awk '
                $1 == "id" { cur_id = $2; sub(/,/, "", cur_id) }
                $0 ~ "node.name = \\"realtime-noise" || $0 ~ "node.description = \\\".*Realtime Noise" {
                    if (cur_id != "") { print cur_id }
                }
            ' | sort -u); do
                pw-cli destroy "$nid" >/dev/null 2>&1 || true
            done
        fi

        rm -f /tmp/hippocamp_pipewire_helper*.lock /tmp/clearcore*.lock /tmp/clearcore_state* /tmp/realtime-noise*.sock 2>/dev/null || true
        rm -f /run/user/*/hippocamp_pipewire_helper*.lock /run/user/*/clearcore_state* /run/user/*/realtime-noise*.sock 2>/dev/null || true
        ;;
    failed-upgrade|upgrade)
        pkill -x "realtime-noise-service" >/dev/null 2>&1 || true
        pkill -x "clearcore-pipewire-helper" >/dev/null 2>&1 || true
        pkill -x "pipewire_helper" >/dev/null 2>&1 || true
        ;;
    *)
        ;;
esac
exit 0
"""
    }
    daemon_deb = os.path.join(out_dir, f"clearcore-daemon_{version}_{arch}.deb")
    build_deb(daemon_deb, daemon_control, daemon_root, scripts_dict=daemon_scripts)

    # 2. GUI files
    os.makedirs(os.path.join(gui_root, "opt/clearcore"), exist_ok=True)
    os.makedirs(os.path.join(gui_root, "usr/bin"), exist_ok=True)
    os.makedirs(os.path.join(gui_root, "usr/share/applications"), exist_ok=True)
    os.makedirs(os.path.join(gui_root, "usr/share/icons/hicolor/512x512/apps"), exist_ok=True)

    exclude_top = {
        "resources", "vendor", "governance", "install.sh", "uninstall.sh",
        "AppRun", "libclearcore_filter.so", "Clearcore-linux-x64.tar.gz",
        "Clearcore-linux-arm64.tar.gz"
    }

    for item in os.listdir(src_dir):
        if item in exclude_top:
            continue
        s = os.path.join(src_dir, item)
        d = os.path.join(gui_root, "opt/clearcore", item)
        if os.path.isdir(s):
            shutil.copytree(s, d)
        else:
            shutil.copy2(s, d)

    if os.path.exists(os.path.join(src_dir, "resources/app")):
        shutil.copytree(os.path.join(src_dir, "resources/app"), os.path.join(gui_root, "opt/clearcore/resources/app"))

    # Create symlinks in opt/clearcore pointing to system clearcore-daemon
    os.makedirs(os.path.join(gui_root, "opt/clearcore/resources/bin"), exist_ok=True)
    os.symlink("/usr/bin/realtime-noise-service", os.path.join(gui_root, "opt/clearcore/resources/bin/realtime-noise-service"))
    os.symlink("/usr/bin/clearcore-pipewire-helper", os.path.join(gui_root, "opt/clearcore/resources/bin/pipewire_helper"))
    os.symlink("/usr/lib/clearcore/libclearcore_filter.so", os.path.join(gui_root, "opt/clearcore/resources/bin/libclearcore_filter.so"))
    os.symlink("/usr/lib/clearcore/libclearcore_filter.so", os.path.join(gui_root, "opt/clearcore/libclearcore_filter.so"))
    os.symlink("/usr/share/clearcore/vendor", os.path.join(gui_root, "opt/clearcore/vendor"))
    os.symlink("/usr/share/clearcore/governance", os.path.join(gui_root, "opt/clearcore/governance"))
    os.symlink("/usr/share/clearcore/scripts", os.path.join(gui_root, "opt/clearcore/resources/scripts"))

    # Desktop icon and launcher
    desktop_file = os.path.join(src_dir, "clearcore.desktop")
    icon_file = os.path.join(src_dir, "clearcore.png")
    if os.path.exists(desktop_file):
        shutil.copy2(desktop_file, os.path.join(gui_root, "usr/share/applications/clearcore.desktop"))
    if os.path.exists(icon_file):
        shutil.copy2(icon_file, os.path.join(gui_root, "usr/share/icons/hicolor/512x512/apps/clearcore.png"))

    wrapper_path = os.path.join(gui_root, "usr/bin/clearcore")
    with open(wrapper_path, "w") as f:
        f.write("#!/bin/sh\nexec /opt/clearcore/clearcore \"$@\"\n")
    os.chmod(wrapper_path, 0o755)

    gui_control = {
        "Package": "clearcore",
        "Version": version,
        "Section": "sound",
        "Priority": "optional",
        "Architecture": arch,
        "Maintainer": "João Rura <joaorura@users.noreply.github.com>",
        "Depends": f"clearcore-daemon (= {version})",
        "Description": "Clearcore Realtime AI Noise Suppression Virtual Microphone\n Desktop GUI and system tray controller for Clearcore.",
    }
    gui_scripts = {
        "prerm": """#!/bin/sh
set -e
pkill -x "clearcore" >/dev/null 2>&1 || true
exit 0
""",
        "postinst": """#!/bin/sh
set -e
gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
update-desktop-database /usr/share/applications 2>/dev/null || true
exit 0
""",
        "postrm": """#!/bin/sh
set -e
gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
update-desktop-database /usr/share/applications 2>/dev/null || true
exit 0
""",
    }
    gui_deb = os.path.join(out_dir, f"clearcore_{version}_{arch}.deb")
    build_deb(gui_deb, gui_control, gui_root, scripts_dict=gui_scripts)

    shutil.rmtree(temp_work, ignore_errors=True)
    return daemon_deb, gui_deb

if __name__ == "__main__":
    src = sys.argv[1] if len(sys.argv) > 1 else "release/Clearcore-linux-x64"
    out = sys.argv[2] if len(sys.argv) > 2 else "release-modular"
    arch = sys.argv[3] if len(sys.argv) > 3 else "amd64"
    ver = sys.argv[4] if len(sys.argv) > 4 else "0.1.0-beta.3"
    print(f"Building modular DEBs for {arch} v{ver} from {src}...")
    d_deb, g_deb = make_modular_debs(src, out, version=ver, arch=arch)
    for p in [d_deb, g_deb]:
        sz = os.path.getsize(p) / (1024 * 1024)
        print(f"✅ Generated {os.path.basename(p)}: {sz:.2f} MB")
