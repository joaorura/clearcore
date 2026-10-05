Name:           clearcore
Version:        0.1.0_beta.3
Release:        1%{?dist}
Summary:        Realtime AI Noise Suppression Virtual Microphone (DeepFilterNet3)
License:        PolyForm Noncommercial 1.0.0
URL:            https://github.com/joaorura/clearcore
Source0:        https://github.com/joaorura/clearcore/archive/refs/tags/v0.1.0-beta.3.tar.gz

ExclusiveArch:  x86_64 aarch64
AutoReqProv:    no
Requires:       pipewire >= 0.3.0

%description
Clearcore is a first-party realtime noise-suppression virtual microphone
powered by DeepFilterNet3 ONNX, PipeWire C bridge, and Rust supervisor daemon.
Provides voice isolation, neural EQ acoustic calibration, and studio DSP.

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}/opt/clearcore
cp -a * %{buildroot}/opt/clearcore/ 2>/dev/null || true

mkdir -p %{buildroot}/usr/bin
cat << 'WRAPPER' > %{buildroot}/usr/bin/clearcore
#!/usr/bin/env bash
exec /opt/clearcore/clearcore "$@"
WRAPPER
chmod 0755 %{buildroot}/usr/bin/clearcore

mkdir -p %{buildroot}/usr/share/applications
cat << 'DESKTOP' > %{buildroot}/usr/share/applications/clearcore.desktop
[Desktop Entry]
Type=Application
Name=Clearcore
GenericName=Noise Suppression Virtual Microphone
Comment=Realtime AI Noise Suppression Virtual Microphone (DeepFilterNet3)
Exec=/opt/clearcore/clearcore
Icon=clearcore
Terminal=false
Categories=AudioVideo;Audio;
Keywords=audio;microphone;noise;filter;clearcore;pipewire;
StartupWMClass=clearcore
DESKTOP

mkdir -p %{buildroot}/usr/share/icons/hicolor/512x512/apps
if [ -f "crates/app-tauri/assets/icon.png" ]; then
    cp "crates/app-tauri/assets/icon.png" %{buildroot}/usr/share/icons/hicolor/512x512/apps/clearcore.png
fi

%files
/opt/clearcore
/usr/bin/clearcore
/usr/share/applications/clearcore.desktop
/usr/share/icons/hicolor/512x512/apps/clearcore.png

%post
gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
update-desktop-database /usr/share/applications 2>/dev/null || true

%postun
gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
update-desktop-database /usr/share/applications 2>/dev/null || true

%changelog
* Mon Oct 05 2026 João Rura <joaorura@users.noreply.github.com> - 0.1.0_beta.3-1
- Official Clearcore multi-platform beta release.
