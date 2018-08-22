#!/usr/bin/python
from __future__ import print_function
import sys
import os
import shutil

def main():
    workspace_folder = sys.argv[1]
    shutil.copy(workspace_folder + '/adapter2/codelldb.py', workspace_folder + '/target/debug/codelldb.py')
    shutil.copy(workspace_folder + '/adapter2/value.py', workspace_folder + '/target/debug/value.py')
    shutil.copy('/usr/lib/llvm-6.0/bin/lldb-server-6.0.1', workspace_folder + '/target/debug/lldb-server-6.0.1')
    shutil.copy('/usr/lib/llvm-6.0/lib/liblldb-6.0.so', workspace_folder + '/target/debug/liblldb-6.0.so')

main()
