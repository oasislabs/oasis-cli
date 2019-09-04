"""Tests `oasis set-toolchain`."""

import os.path as osp
import subprocess
from subprocess import PIPE
import sys

import pytest

MOCK_SERVER_PY = osp.join(osp.dirname(__file__), 'mock_tools_server.py')


def test_set_toolchain_error(oenv):
    cp = oenv.run('oasis set-toolchain blah', input='', stderr=PIPE, check=False)
    assert 'unknown toolchain' in cp.stderr


@pytest.fixture(scope='package')
def tools_proxy():
    """Starts an HTTP server that mocks https://tools.oasis.dev"""
    cp = subprocess.Popen([sys.executable, MOCK_SERVER_PY], stdout=PIPE)
    port = cp.stdout.readline().rstrip().decode('utf8')
    yield f'http://localhost:{port}'
    cp.terminate()
    cp.wait()


def test_set_toolchain_latest_unstable(oenv, tools_proxy, mock_tool):
    env = {'http_proxy': tools_proxy}
    oenv.run('oasis set-toolchain unstable', input='', env=env)
    cp = oenv.run('oasis-chain', stdout=PIPE)
    invocation = mock_tool.parse_output(cp.stdout)[0]
    assert invocation['name'] == osp.join(oenv.bin_dir, 'oasis-chain')
    assert invocation['user'] == f'{sys.platform} current oasis-chain 1111111'


def test_set_toolchain_latest(oenv, tools_proxy, mock_tool):
    env = {'http_proxy': tools_proxy}
    oenv.run('oasis set-toolchain latest', input='', env=env)
    cp = oenv.run('oasis-chain', stdout=PIPE)
    invocation = mock_tool.parse_output(cp.stdout)[0]
    assert invocation['name'] == osp.join(oenv.bin_dir, 'oasis-chain')
    assert invocation['user'] == f'{sys.platform} 20.19 oasis-chain 0fedcba'


def test_set_toolchain_named(oenv, tools_proxy, mock_tool):
    env = {'http_proxy': tools_proxy}
    oenv.run('oasis set-toolchain 19.20', input='', env=env)
    cp = oenv.run('oasis-chain', stdout=PIPE)
    invocation = mock_tool.parse_output(cp.stdout)[0]
    assert invocation['name'] == osp.join(oenv.bin_dir, 'oasis-chain')
    assert invocation['user'] == f'{sys.platform} 19.20 oasis-chain abcdef0'
