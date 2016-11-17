"""Module for regression testing RUPS"""
from __future__ import print_function
from sys import argv, stdout
from os import path
import argparse
import threading
import subprocess
import time
import signal
import pprint # Dev import
import telnetlib

RUPS_BIN = path.join('target', 'debug', 'rups')
TELNET_BIN = 'telnet'

def single_controller(port, puts, expected):
    """Start a single controlling process"""
    print("Launching controller")
    client = telnetlib.Telnet('127.0.0.1', port)
    if puts:
        for line in puts:
            client.write(line)
            client.write('\r')

    lines = client.read_all().split('\r\n')
    # Skip 5 lines, motd
    for (line, out) in zip(expected, lines[5:]):
        #print(":".join("{:02x}".format(ord(c)) for c in out))
        if len(out) == 0:
            print("got 0 length string..")
        if out != line:
            print("fail {} != {}".format(out, line))
        else:
            print("success {} == {}".format(out, line))

def single_logger(port, expected):
    """Start a single logging process"""
    print("Launching logger")
    client = telnetlib.Telnet('127.0.0.1', port)
    lines = client.read_all().split('\r\n')
    # Skip 5 lines, motd
    for (line, out) in zip(expected, lines[5:]):
        #print(":".join("{:02x}".format(ord(c)) for c in out))
        if out != line:
            print("fail {} != {}".format(out, line))
        else:
            print("success {} == {}".format(out, line))

def execute_test(n_controllers, n_loggers):
    """Test function"""
    # Run RUPS
    server = subprocess.Popen([path.join(RUPS_BIN), '--noinfo', 'cat', '--bind', '3000', '--logbind',
                               '4000'],
                              stdout=stdout.fileno(), stdin=subprocess.PIPE)
    time.sleep(1)

    stimuli = ['Hello world!']
    expected = stimuli[:] + stimuli[:]

    # Run first controller that will output something
    threads = []
    thread = threading.Thread(target=single_controller,
                              args=('3000', stimuli, expected))
    thread.start()
    threads.append(thread)
    # Run the rest of the controllers
    for _ in range(n_controllers-1):
        thread = threading.Thread(target=single_controller,
                                  args=('3000', None, expected))
        thread.start()
        threads.append(thread)

    # Run loggers
    for _ in range(n_loggers):
        thread = threading.Thread(target=single_logger, args=('4000', expected))
        thread.start()
        threads.append(thread)

    time.sleep(3)
    print("Sending INTERRUPT to server")
    server.send_signal(signal.SIGINT)
    server.wait()

    for thread in threads:
        thread.join()

    print('Test complete')


def main(args):
    """Main function"""
    parser = argparse.ArgumentParser(description='Test multiple connections')
    parser.add_argument('-n', help='Number of control telnet clients', default=1, type=int)
    parser.add_argument('-m', help='Number of log telnet clients', default=1, type=int)

    args = parser.parse_args(args)

    execute_test(args.n, args.m)

if __name__ == "__main__":
    main(argv[1:])
