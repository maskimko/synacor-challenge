Synacor challenge
=================

This repository contains 2 version of the OSCONN Synacor challenge.
Those ones I found in the open internet. It looks like those are just a different versions of the same challenge, but I did not know which one works, so I decided to save both of them. 

Hence there are two directories var1 and var2 with different version of the challenge binaries. 

The rules of the challenge are described in the file called *arch-spec* 
The binary program is a data file of approximately 60kb size called *challenge.bin*. 

The goal is to collect all the codes, meanwhile solving quizes, mazes, puzzles, and other kinds of programming (and not only) challenges. 

I started to implement things from _var1_ version. My solution is in the _solution_ directory. 
Also there is a python file called *compile_test.py*, which compiles a small *sample.bin* binary file, which can be used to check the initial implementation of the VM. The code of this file I actually  found in the *arch-spec* file, but there were no convenient way to test if this program works. 
So feel free to use this script to generate a testing sample of the binary code. 

## Rust notes

### Prerequisites

To build the code you should have these 2 programs installed:
* cargo - Rust package manager
* rustc - Rust compiler

The most convenient way to install rust is to use _rustup_ tool. 
Please, visit https://rustup.rs/ for further instructions

### Build

Just run:
> cargo build

### Run

The *challenge.bin* file should be present in the crate directory (your working directory). 
And then just run: 
> cargo run

Also it is possible to specify the location of the challenge binary file like this:
> cargo run -- --rom ./challenge.bin

For other options run:
> cargo run -- --help
The help is pretty self explanatory

#### Note

It is convenient to have color output for debugging, but it is also very useful to have a pager program, when debugging. 
To preserve colors in less, while debugging, you can hit this command: 
 > CLICOLOR_FORCE=1 RUST_LOG=trace cargo run -- --rom sample.bin 2>&1 | less -R 
It will show you an example of colored output in the _less_ pager. Alos _bat_ works like a charm ;)


Good luck, with solving it your way! 

## Acknowledgements

### Dave Eddy 
Thank you for inspiring me to solve this challenge on your Christmas stream! 
You can reach out to him by [his personal web page](https://www.daveeddy.com/)

### Eduard Mandy
His [github repo](https://github.com/mandyedi/synacor-challenge), is the first version of the challenge I found. 

