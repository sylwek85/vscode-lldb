#!/usr/bin/python
from __future__ import print_function
import sys
import os
import shutil
import subprocess

def main():
    subprocess.check_call(['cargo', 'build'])

    workspace_folder = sys.argv[1]
    build_dir = workspace_folder + '/target/debug'
    target_dir = workspace_folder + '/out/adapter2'
    shutil.rmtree(target_dir, ignore_errors=True)
    makedirs(target_dir)

    shutil.copy2(workspace_folder + '/adapter2/codelldb.py', target_dir)
    shutil.copy2(workspace_folder + '/adapter2/rust.py', target_dir)
    shutil.copy2(workspace_folder + '/adapter2/value.py', target_dir)
    if sys.platform.startswith('linux'):
        shutil.copy2(build_dir + '/codelldb', target_dir)
        shutil.copy2(build_dir + '/libcodelldb.so', target_dir)
        shutil.copy2('/usr/lib/llvm-6.0/bin/lldb-server-6.0.1', target_dir)
        shutil.copy2('/usr/lib/llvm-6.0/lib/liblldb-6.0.so', target_dir)
    elif sys.platform.startswith('darwin'):
        shutil.copy2(build_dir + '/codelldb', target_dir)
        shutil.copy2(build_dir + '/libcodelldb.dylib', target_dir)
        shutil.copytree('/Library/Developer/CommandLineTools/Library/PrivateFrameworks/LLDB.framework', target_dir + '/LLDB.framework')
    elif sys.platform.startswith('win32'):
        shutil.copy2(build_dir + '/codelldb.exe', target_dir)
        shutil.copy2(build_dir + '/codelldb.dll', target_dir)
        shutil.copy2('C:/NW/ll/build/bin/liblldb.dll', target_dir)
        target_site_packages = target_dir + '/../lib/site-packages'
        shutil.rmtree(target_site_packages, ignore_errors=True)
        shutil.copytree('C:/NW/ll/build/lib/site-packages', target_site_packages)
    else:
        assert False

def makedirs(path):
    try:
        os.makedirs(path)
    except OSError as err:
        pass

main()
