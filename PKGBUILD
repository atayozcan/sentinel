# Maintainer: Atay Özcan <atay@oezcan.me>
pkgname=sentinel
pkgver=0.1.0
pkgrel=1
pkgdesc="Windows UAC-like confirmation dialog for Linux privilege escalation"
arch=('x86_64')
url="https://github.com/atayozcan/sentinel"
license=('GPL-3.0-or-later')
depends=('pam' 'gtk4' 'libadwaita' 'gtk4-layer-shell')
makedepends=('meson' 'ninja')
backup=('etc/security/sentinel.conf' 'etc/pam.d/polkit-1')
install=sentinel.install

build() {
    cd "$startdir"
    rm -rf build
    meson setup build --prefix=/usr --sysconfdir=/etc --libdir=/usr/lib --libexecdir=lib
    meson compile -C build
}

package() {
    cd "$startdir"
    meson install -C build --destdir="$pkgdir"
}
