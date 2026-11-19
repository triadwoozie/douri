# Binary RPM for Dōri
%global debug_package %{nil}
%global __strip /bin/true
%global _build_id_links none

Name:           douri
Version:        %{pkg_version}
Release:        1
Summary:        Music production studio in the style of Logic Pro
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
