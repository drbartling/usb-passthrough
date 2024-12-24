#!/usr/bin/python3
import glob

import serial

ports = glob.glob("/dev/ttyACM*")
ports.extend(glob.glob("/dev/ttyUSB*"))
baud_rate = 115200
test_str = b",".join([f"World{i}".encode("utf-8") for i in range(0, 474)])

for com_port in ports:
    with serial.Serial(com_port, baud_rate, timeout=0.2) as ser:
        ser.flushInput()
        ser.reset_input_buffer()
        ser.write(test_str)
        rsp = ser.read(99999)
        print(test_str)
        print(rsp)
        assert test_str == rsp, f"{com_port} failed check"
