"""Utilities for testing the Oasis CLI."""
import collections
import os
import os.path as osp
import subprocess
import tempfile

import pytest

TARGET_DIR = osp.abspath(osp.join(osp.dirname(__file__), '..', 'target', 'debug'))

OasisEnv = collections.namedtuple('OasisEnv',
                                  'home_dir config_dir data_dir config_file metrics_file')


@pytest.fixture(params=[None, 'custom_prefix'])
def oenv(request):
    orig_envs = dict(os.environ)
    orig_home = orig_envs['HOME']
    orig_cwd = os.curdir
    with tempfile.TemporaryDirectory() as tempdir:
        os.chdir(tempdir)

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

        oasis_config_dir = osp.join(config_dir, 'oasis')
        oasis_data_dir = osp.join(data_dir, 'oasis')

        yield OasisEnv(
            home_dir=tempdir,
            config_dir=oasis_config_dir,
            data_dir=oasis_data_dir,
            config_file=osp.join(oasis_config_dir, 'config.toml'),
            metrics_file=osp.join(oasis_data_dir, 'metrics.jsonl'))

        os.chdir(orig_cwd)  # must chdir before tempdir is deleted
    os.environ = orig_envs


@pytest.fixture
def default_config():
    assert os.environ['HOME'].startswith(
        '/tmp'), 'default_config fixture should come after oenv fixture'
    run('oasis')


@pytest.fixture
def telemetry_config():
    assert os.environ['HOME'].startswith(
        '/tmp'), 'default_config fixture should come after oenv fixture'
    run('oasis', input='y')


def run(cmd, check=True, envs=None, **run_args):
    if envs:
        env = dict(os.environ)
        env.update(envs)
    else:
        env = os.environ
    return subprocess.run(cmd, shell=True, env=env, check=check, encoding='utf8', **run_args)
