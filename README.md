# pythonvm-rust

[![Build Status](https://travis-ci.org/ProgVal/pythonvm-rust.svg?branch=master)](https://travis-ci.org/ProgVal/pythonvm-rust)

A Python virtual machine, written in Rust.

## Status

This project is inactive. Check out [RustPython](https://github.com/RustPython/RustPython/) instead

## Features

* prints strings to stdout
* basic exceptions
* for loops
* functions, positional arguments, keyword arguments, `*args`, `**kwargs`
* useable as a library
* a fine-grained sandbox

## Goals

* Compatible with CPython 3.6's bytecode, in order to take advantage of [FAT Python](https://faster-cpython.readthedocs.org/fat_python.html)
* Support CPython's implementation of the standard library
* No crash, even when messing with code objects
* Bytecode optimizations at runtime
* Less bounded by the GIL than CPython

## Dependencies

* CPython 3.6 (used as a parser and bytecode compiler).
* [Rust](https://www.rust-lang.org/downloads.html)
* [Cargo](https://crates.io/install)

## Try it

1. `git clone https://github.com/progval/pythonvm-rust.git`
2. `cd pythonvm-rust`
3. `python3 -m compileall -b pythonlib examples`
4. `cargo run pythonlib/ examples/helloworld.pyc`
