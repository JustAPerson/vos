# Vos
[![Join the chat at https://gitter.im/JustAPerson/vos](https://badges.gitter.im/JustAPerson/vos.svg)](https://gitter.im/JustAPerson/vos?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge)

Vos is a toy operating system written in Rust.
# Prerequisites

* [Rust nightly](http://www.rust-lang.org/)
* [binutils](http://www.gnu.org/software/binutils/)
* [qemu](http://wiki.qemu.org/Main_Page)

## Ubuntu

You can install rust from by building the [source code][rsrc], or you
can fetch and execute the simple script below:

```
curl -sSf https://static.rust-lang.org/rustup.sh | sh -s -- --channel=nightly
sudo apt-get install binutils qemu
```

[rsrc]: https://github.com/rust-lang/rust#building-from-source

# Usage

Use `make` or `make run` to build and execute Vos using qemu.

