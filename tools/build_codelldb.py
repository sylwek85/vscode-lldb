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
    makedirs(target_dir)

    shutil.copy(workspace_folder + '/adapter2/codelldb.py', target_dir)
    shutil.copy(workspace_folder + '/adapter2/value.py', target_dir)
    if sys.platform.startswith('linux'):
        shutil.copy(build_dir + '/codelldb', target_dir)
        shutil.copy(build_dir + '/libcodelldb.so', target_dir)
        shutil.copy('/usr/lib/llvm-6.0/bin/lldb-server-6.0.0', target_dir)
        shutil.copy('/usr/lib/llvm-6.0/lib/liblldb-6.0.so', target_dir)
    elif sys.platform.startswith('darwin'):
        pass
    elif sys.platform.startswith('win32'):
        shutil.copy(build_dir + '/codelldb.exe', target_dir)
        shutil.copy(build_dir + '/libcodelldb.dll', target_dir)
        shutil.copy('C:/NW/ll/build/bin/liblldb.dll', target_dir)
        target_site_packages = target_dir + '/../lib/site-packages'
        shutil.rmtree(target_site_packages)
        shutil.copytree('C:/NW/ll/build/lib/site-packages', target_site_packages)
    else:
        assert False

def makedirs(path):
    try:
        os.makedirs(path)
    except OSError as err:
        pass

main()
