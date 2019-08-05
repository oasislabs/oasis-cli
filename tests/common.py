"""Utilities for testing the Oasis CLI."""
import os
import os.path as osp
import subprocess
from subprocess import DEVNULL
import tempfile
import toml

import pytest

TARGET_DIR = osp.abspath(osp.join(osp.dirname(__file__), '..', 'target', 'debug'))


class OasisEnv:
    """Provides information about the virtual user environment in which
       the CLI is currently running."""
    def __init__(self, home_dir, user_config_dir, user_data_dir):
        self.home_dir = home_dir
        self.config_dir = osp.join(user_config_dir, 'oasis')
        self.data_dir = osp.join(user_data_dir, 'oasis')
        self.config_file = osp.join(self.config_dir, 'config.toml')
        self.metrics_file = osp.join(self.data_dir, 'metrics.jsonl')

    def load_config(self):
        with open(self.config_file) as f_config:
            return toml.load(f_config)

    def run(self, *args, **kwargs):
        return _run(*args, cwd=self.home_dir, **kwargs)


@pytest.fixture(params=[None, 'custom_prefix'])
def oenv(request):
    orig_envs = dict(os.environ)
    orig_home = orig_envs['HOME']
    orig_cwd = os.curdir
    with tempfile.TemporaryDirectory() as tempdir:
        os.environ['HOME'] = tempdir
        os.environ['CARGO_HOME'] = osp.join(orig_home, '.cargo')
        os.environ['RUSTUP_HOME'] = osp.join(orig_home, '.rustup')
        os.environ['PATH'] = TARGET_DIR + ':' + os.environ['PATH']

        if request.param:
            config_dir = osp.abspath(osp.join(tempdir, request.param, 'config'))
            data_dir = osp.abspath(osp.join(tempdir, request.param, 'data'))
            os.environ['XDG_CONFIG_HOME'] = config_dir
            os.environ['XDG_DATA_HOME'] = data_dir
        else:
            config_dir = osp.join(tempdir, '.config')
            data_dir = osp.join(tempdir, '.local', 'share')

        yield OasisEnv(tempdir, config_dir, data_dir)

        os.chdir(orig_cwd)  # must chdir before tempdir is deleted
    os.environ = orig_envs


@pytest.fixture
def default_config():
    assert os.environ['HOME'].startswith(
        '/tmp'), 'default_config fixture should come after oenv fixture'
    _run('oasis', stdout=DEVNULL, stderr=DEVNULL)


@pytest.fixture
def telemetry_config():
    assert os.environ['HOME'].startswith(
        '/tmp'), 'default_config fixture should come after oenv fixture'
    _run('oasis', input='y', stdout=DEVNULL, stderr=DEVNULL)


def _run(cmd, check=True, envs=None, **run_args):
    if envs:
        env = dict(os.environ)
        env.update(envs)
    else:
        env = os.environ
    return subprocess.run(cmd, shell=True, env=env, check=check, encoding='utf8', **run_args)
