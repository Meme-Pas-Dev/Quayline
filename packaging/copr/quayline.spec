# Paquet du dépôt COPR (matt-mpd/quayline), compilé depuis les sources par COPR.
# @VERSION@ et @BUILD@ sont remplacés par le workflow .github/workflows/copr.yml ; le
# nom et le numéro de release suivent ceux du RPM des releases GitHub, pour que l'un
# remplace l'autre avec « dnf upgrade ».
# COPR : « Enable internet access during builds » (dépendances npm et cargo).

%global debug_package %{nil}

Name:           quayline
Version:        @VERSION@
Release:        @BUILD@%{?dist}
Summary:        Secure SSH/SFTP client with an encrypted vault
License:        GPL-3.0-or-later
URL:            https://github.com/Meme-Pas-Dev/quayline
Source0:        %{name}-%{version}.tar.gz
ExclusiveArch:  x86_64

BuildRequires:  gcc
BuildRequires:  gcc-c++
BuildRequires:  make
BuildRequires:  pkgconf-pkg-config
BuildRequires:  rust >= 1.80
BuildRequires:  cargo
BuildRequires:  nodejs
BuildRequires:  npm
BuildRequires:  webkit2gtk4.1-devel
BuildRequires:  openssl-devel
BuildRequires:  librsvg2-devel
BuildRequires:  systemd-devel
BuildRequires:  desktop-file-utils

%description
Quayline is a security-focused SSH and SFTP client. Servers, credentials and keys
are kept in an encrypted vault (Argon2id, XChaCha20-Poly1305), unlocked with a master
password, a FIDO2 security key or an OpenID Connect identity provider.

%prep
%autosetup

%build
npm ci --no-audit --no-fund
npx tauri build --no-bundle -- --locked

%install
install -Dm755 target/release/%{name} %{buildroot}%{_bindir}/%{name}
sed -e 's/{{name}}/Quayline/g' -e 's/{{comment}}/Client SSH\/SFTP sécurisé/' \
    -e 's/{{exec}}/%{name}/' -e 's/{{icon}}/%{name}/' \
    src-tauri/linux/app.desktop > Quayline.desktop
desktop-file-install --dir=%{buildroot}%{_datadir}/applications Quayline.desktop
install -Dm644 src-tauri/icons/32x32.png %{buildroot}%{_datadir}/icons/hicolor/32x32/apps/%{name}.png
install -Dm644 src-tauri/icons/128x128.png %{buildroot}%{_datadir}/icons/hicolor/128x128/apps/%{name}.png
install -Dm644 src-tauri/icons/128x128@2x.png %{buildroot}%{_datadir}/icons/hicolor/256x256@2/apps/%{name}.png
install -Dm644 src-tauri/icons/icon.png %{buildroot}%{_datadir}/icons/hicolor/512x512/apps/%{name}.png

%files
%license LICENSE
%{_bindir}/%{name}
%{_datadir}/applications/Quayline.desktop
%{_datadir}/icons/hicolor/*/apps/%{name}.png

%changelog
* @DATE@ Quayline <noreply@github.com> - @VERSION@-@BUILD@
- Version @VERSION@ (see https://github.com/Meme-Pas-Dev/quayline/releases)
