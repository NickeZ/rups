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

TOP_DIR = path.join('..')
RUPS_BIN = path.join('target', 'debug', 'rups')
TELNET_BIN = 'telnet'

def single_controller(port, puts, expected):
    """Start a single controlling process"""
    print("Launching controller")
    proc = subprocess.Popen([TELNET_BIN, '127.0.0.1', port], stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE)
    if puts:
        for line in puts:
            proc.stdin.write(line)

    for line in expected:
        out = proc.stdout.readline()
        if out != line:
            print("{} != {}".format(out, line))

    proc.wait()

def single_logger(port, expected):
    """Start a single logging process"""
    print("Launching logger")
    proc = subprocess.Popen([TELNET_BIN, '127.0.0.1', port], stdout=subprocess.PIPE)
    for line in expected:
        out = proc.stdout.readline()
        if out != line:
            print("{} != {}".format(out, line))

    proc.wait()

def execute_test(n_controllers, n_loggers):
    """Test function"""
    # Run RUPS
    server = subprocess.Popen([path.join(TOP_DIR, RUPS_BIN), 'cat', '--bind', '3000', '--logbind',
                               '4000'],
                              stdout=stdout.fileno(), stdin=subprocess.PIPE)
    time.sleep(2)

    stimuli = ['Hello world!']
    expected = ["Trying 127.0.0.1...\n", "Connected to 127.0.0.1.\n", "Escape character is '^]'.\n"]
    expected.extend(stimuli)

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

    time.sleep(10)
    print("Sending INTERRUPT to server")
    server.send_signal(signal.SIGINT)
    server.wait()

    for thread in threads:
        thread.join()

    print('Test complete')


def main(args):
    """Main function"""
    parser = argparse.ArgumentParser(description='Test multiple connections')
    parser.add_argument('-n', help='Number of control telnet clients', default=1)
    parser.add_argument('-m', help='Number of log telnet clients', default=1)

    args = parser.parse_args(args)

    execute_test(args.n, args.m)

if __name__ == "__main__":
    main(argv[1:])
