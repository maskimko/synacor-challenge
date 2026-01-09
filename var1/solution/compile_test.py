#!/usr/bin/env python3

# arch-spec contains an exmaple of this program. But it should be compiled
# 
# - The program "9,32768,32769,4,19,32768" occupies six memory addresses and should:
#   - Store into register 0 the sum of 4 and the value contained in register 1.
#   - Output to the terminal the character with the ascii code contained in register 0.
import sys
from typing import Tuple


#TEST_LINE = "9,32768,32769,4,19,32768" 
# I changed this line to output '!' instead of ASCII symbol 4
TEST_LINE = "9,32768,32769,33,19,32768" 

def to_little_endian(num: int) -> Tuple[int, int]:
    lb = num % (1 << 8);
    hb = num >> 8;
    return (lb, hb)

def compile(prog: str, out_file: str): 
    chunks = prog.split(",")
    numbers = []
    for c in chunks:
        lb, hb = to_little_endian(int(c))
        numbers.append(lb)
        numbers.append(hb)
    print(f"Numbers of the program {numbers}" )
    byte_array = bytes(numbers)
    print(f"Byte array {byte_array}")
    with open(out_file, "wb") as f:
        f.write(byte_array)



if __name__ == "__main__":
    print("Compiling the test line")
    out_file = "sample.bin"
    if len(sys.argv) == 2:
        out_file = sys.argv[1]
    compile(TEST_LINE,out_file)
    print(f"Compiled program is available at {out_file}")
