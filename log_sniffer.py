# /// script
# dependencies = ["pyserial"]
# ///

import serial
import subprocess
import sys

ELF = r".\target\thumbv7em-none-eabihf\debug\examples\defmt_serial"
PORT = "COM5"
BAUD = 115200

proc = subprocess.Popen(
    ["defmt-print", "-e", ELF],
    stdin=subprocess.PIPE,
    stdout=sys.stdout,
    stderr=sys.stderr,
)

with serial.Serial(PORT, BAUD) as s:
    while True:
        data = s.read(64)
        if data:
            proc.stdin.write(data)
            proc.stdin.flush()