#!/usr/bin/python3
import glob

import serial

ports = glob.glob("/dev/ttyACM*")
ports.extend(glob.glob("/dev/ttyUSB*"))
baud_rate = 115200
test_str = b",".join([f"World{i}".encode("utf-8") for i in range(0, 20)])

for com_port in ports:
    with serial.Serial(com_port, baud_rate, timeout=0.2) as ser:
        ser.flushInput()
        ser.reset_input_buffer()
        ser.write(test_str)
        rsp = ser.read(9999)
        print(f"{com_port}\r\n\t{rsp}")
