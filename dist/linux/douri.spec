# Binary RPM for Fedora/openSUSE/RHEL 9+ (x86_64, glibc 2.31+). It repackages the release
# binary built by CI instead of compiling, so build it with:
#   rpmbuild -bb dist/linux/douri.spec --define "_sourcedir <dir with douri and
#   the dist/linux files>" --define "pkg_version <version>"
%global debug_package %{nil}
%global __strip /bin/true
%global _build_id_links none

Name:           dori
Version:        %{pkg_version}
Release:        1
Summary:        Pure Rust digital audio workstation
License:        GPL-3.0-or-later
URL:            https://github.com/triadwoozie/douri
ExclusiveArch:  x86_64
Source0:        douri
Source1:        io.github.triadwoozie.douri.desktop
Source2:        io.github.triadwoozie.douri.png
Source3:        douri-mime.xml
Source4:        README.txt

Recommends:     libxkbcommon-x11
Recommends:     libwayland-client
Recommends:     mesa-libEGL
Recommends:     fontconfig
Recommends:     xdg-desktop-portal
Suggests:       mesa-vulkan-drivers
Suggests:       zenity

%description
Dōri is a digital audio workstation written in Rust: tracks, regions, a piano
roll, a mixer and built-in synthesizer and sampling instruments, with audio through PipeWire/ALSA.

%prep

%build

%install
install -D -m 755 %{SOURCE0} %{buildroot}%{_bindir}/douri
install -D -m 644 %{SOURCE1} %{buildroot}%{_datadir}/applications/io.github.triadwoozie.douri.desktop
install -D -m 644 %{SOURCE2} %{buildroot}%{_datadir}/icons/hicolor/512x512/apps/io.github.triadwoozie.douri.png
install -D -m 644 %{SOURCE3} %{buildroot}%{_datadir}/mime/packages/douri.xml
install -D -m 644 %{SOURCE4} %{buildroot}%{_docdir}/%{name}/README.txt

%files
%{_bindir}/douri
%{_datadir}/applications/io.github.triadwoozie.douri.desktop
%{_datadir}/icons/hicolor/512x512/apps/io.github.triadwoozie.douri.png
%{_datadir}/mime/packages/douri.xml
%doc %{_docdir}/%{name}/README.txt
