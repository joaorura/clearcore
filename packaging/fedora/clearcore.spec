%global tag_version v0.1.0-beta.2

Name:           clearcore
Version:        0.1.0_beta.2
Release:        3%{?dist}
Summary:        Realtime AI Noise Suppression Virtual Microphone (DeepFilterNet3)
License:        PolyForm Noncommercial 1.0.0
URL:            https://github.com/joaorura/clearcore
Source0:        LICENSE
Source1:        clearcore.desktop
Source2:        clearcore.png

ExclusiveArch:  x86_64 aarch64
AutoReqProv:    no
BuildRequires:  curl, tar
Requires:       pipewire >= 0.3.0
# AutoReqProv is off (the bundle ships its own libs), so the runtime libraries
# needed by the Electron UI, the Rust daemon and the PipeWire helper are listed
# explicitly. Derived from the NEEDED entries of the packaged ELF files.
Requires:       alsa-lib
Requires:       at-spi2-atk
Requires:       at-spi2-core
Requires:       atk
Requires:       cairo
Requires:       cups-libs
Requires:       dbus-libs
Requires:       expat
Requires:       glib2
Requires:       gtk3
Requires:       libX11
Requires:       libXcomposite
Requires:       libXdamage
Requires:       libXext
Requires:       libXfixes
Requires:       libXrandr
Requires:       libgcc
Requires:       libstdc++
Requires:       libxcb
Requires:       libxkbcommon
Requires:       mesa-libgbm
Requires:       nspr
Requires:       nss
Requires:       nss-util
Requires:       pango
Requires:       pipewire-libs
Requires:       systemd-libs

%description
Clearcore is a first-party realtime noise-suppression virtual microphone
powered by DeepFilterNet3 ONNX, PipeWire C bridge, and Rust supervisor daemon.
Provides voice isolation, neural EQ acoustic calibration, and studio DSP.

%prep
mkdir -p dist
%ifarch x86_64
curl -fsSL --retry 3 --retry-delay 5 -o bundle.tar.gz https://github.com/joaorura/clearcore/releases/download/%{tag_version}/Clearcore-linux-x64.tar.gz
%endif
%ifarch aarch64
curl -fsSL --retry 3 --retry-delay 5 -o bundle.tar.gz https://github.com/joaorura/clearcore/releases/download/%{tag_version}/Clearcore-linux-arm64.tar.gz
%endif
tar -xzf bundle.tar.gz -C dist --strip-components=1

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}/opt/clearcore
cp -a dist/* %{buildroot}/opt/clearcore/

mkdir -p %{buildroot}/usr/bin
cat << 'WRAPPER' > %{buildroot}/usr/bin/clearcore
#!/usr/bin/env bash
exec /opt/clearcore/clearcore "$@"
WRAPPER
chmod 0755 %{buildroot}/usr/bin/clearcore

mkdir -p %{buildroot}/usr/share/applications
cp %{SOURCE1} %{buildroot}/usr/share/applications/clearcore.desktop

mkdir -p %{buildroot}/usr/share/icons/hicolor/512x512/apps
cp %{SOURCE2} %{buildroot}/usr/share/icons/hicolor/512x512/apps/clearcore.png

%files
/opt/clearcore
/usr/bin/clearcore
/usr/share/applications/clearcore.desktop
%{_datadir}/icons/hicolor/512x512/apps/clearcore.png

%post
gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
update-desktop-database /usr/share/applications 2>/dev/null || true

%postun
gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
update-desktop-database /usr/share/applications 2>/dev/null || true

%changelog
* Mon Nov 09 2026 João Rura <joaorura@users.noreply.github.com> - 0.1.0_beta.2-3
- Declare Electron/GTK/NSS runtime dependencies (AutoReqProv is off).

* Mon Nov 09 2026 João Rura <joaorura@users.noreply.github.com> - 0.1.0_beta.2-2
- Rebuild of v0.1.0-beta.2 with enrollment fixes: active-speech level gate when building the voice profile, bounded denoiser attenuation of stored samples, visible running enrollment jobs, dBFS level meter and microphone label in the UI.

* Fri Oct 16 2026 João Rura <joaorura@users.noreply.github.com> - 0.1.0_beta.2-1
- Release v0.1.0-beta.2: fix voice profile enrollment noisy-environment gate.

* Fri Oct 09 2026 João Rura <joaorura@users.noreply.github.com> - 0.1.0_beta.6-1
- Release v0.1.0-beta.6: Bundle native voice enrollment model into release packages, robust multi-path model lookup.

* Thu Oct 08 2026 João Rura <joaorura@users.noreply.github.com> - 0.1.0_beta.4-1
- Release v0.1.0-beta.4: pDFNet3 Pro integration, Dual-Conditioning, Voice Isolation & Dynamic AGC Leveler.

* Mon Oct 05 2026 João Rura <joaorura@users.noreply.github.com> - 0.1.0_beta.1-1
- Official Clearcore multi-platform beta release.
