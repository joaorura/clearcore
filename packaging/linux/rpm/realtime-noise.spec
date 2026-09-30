Name:           realtime-noise
Version:        0.1.0
Release:        1%{?dist}
Summary:        High-performance low-latency realtime noise suppression platform

License:        Apache-2.0
URL:            https://clearcore.com/realtime-noise
Source0:        realtime-noise-%{version}.tar.gz

Requires:       pipewire >= 0.3.0
Requires:       wireplumber >= 0.4.0

%description
Clearcore Realtime Noise provides continuous noise suppression with < 10ms
processing latency via PipeWire native integration and tract inference engine.

%install
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_userunitdir}
install -m 0755 target/release/realtime-noise-service %{buildroot}%{_bindir}/realtime-noise-service
install -m 0644 packaging/linux/systemd/user/realtime-noise.service %{buildroot}%{_userunitdir}/realtime-noise.service

%files
%{_bindir}/realtime-noise-service
%{_userunitdir}/realtime-noise.service

%changelog
* Wed Sep 30 2026 Clearcore Packaging Team <packaging@clearcore.com> - 0.1.0-1
- Initial production release
