#!/usr/bin/env python3

import os
import glob
import subprocess

SOURCE_DIR = os.path.dirname('__file__')
BIN = os.path.join(SOURCE_DIR, 'target', 'debug', 'pythonvm')
LIB_DIR = os.path.join(SOURCE_DIR, 'pythonlib')
TESTS_DIR = os.path.join(SOURCE_DIR, 'functional_tests')

subprocess.check_call(['/usr/bin/env', 'python3', '-m', 'compileall', '-b', TESTS_DIR],
        stdout=subprocess.DEVNULL)

all_ok = True

for filename in glob.glob(TESTS_DIR + os.path.sep + '*.py'):
    vm_result = subprocess.check_output([BIN, LIB_DIR, filename + 'c'])
    system_python_result = subprocess.check_output(['/usr/bin/env', 'python3', filename])
    if vm_result != system_python_result:
        print('=' * 100)
        print('Test {} failed.'.format(filename))
        print('-' * 100)
        print('System Python:')
        print(system_python_result)
        print('-' * 100)
        print('VM result:')
        print(vm_result)
        print('=' * 100)
        all_ok = False

if all_ok:
    exit(0)
else:
    exit(1)
