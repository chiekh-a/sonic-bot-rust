export OPENSSL_DIR=$(nix eval nixpkgs.openssl --raw)
export OPENSSL_INCLUDE_DIR=$OPENSSL_DIR/include
export OPENSSL_LIB_DIR=$OPENSSL_DIR/lib
export PKG_CONFIG_PATH=$OPENSSL_DIR/lib/pkgconfig
export PKG_CONFIG_ALLOW_CROSS=1
export OPENSSL_NO_VENDOR=1
nix-shell -p openssl pkg-config --run "cargo build --release"

