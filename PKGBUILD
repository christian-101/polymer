# Maintainer: Your Name <your.email@example.com>
pkgname=polymer-git
pkgver=0.1.0.r0.g$(git rev-parse --short HEAD)
pkgrel=1
pkgdesc="A TUI deployment dashboard for Vercel"
arch=('x86_64')
url="https://github.com/christian-101/polymer"
license=('MIT')
depends=('openssl')
makedepends=('cargo' 'git')
provides=("${pkgname%-git}")
conflicts=("${pkgname%-git}")
source=("git+${url}.git")
md5sums=('SKIP')

pkgver() {
    cd "$srcdir"
    git describe --long --tags 2>/dev/null | sed 's/\([^-]*-g\)/r\1/;s/-/./g' || \
    printf "r%s.%s" "$(git rev-list --count HEAD)" "$(git rev-parse --short HEAD)"
}

build() {
    cd "$srcdir"
    cargo build --release --locked --target-dir=target
}

package() {
    cd "$srcdir"
    install -Dm755 target/release/polymer "$pkgdir/usr/bin/polymer"
    install -Dm644 README.md "$pkgdir/usr/share/doc/$pkgname/README.md"
}
