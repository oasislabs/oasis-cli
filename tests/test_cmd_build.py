"""Tests `oasis build`."""

import os.path as osp
from subprocess import PIPE


def test_cargo_version(oenv, mock_tool):
    mock_tool.create_at(osp.join(oenv.bin_dir, 'cargo'))
    svc_dir = osp.join(oenv.create_project(), 'service')
    cp = oenv.run('oasis build', cwd=svc_dir, stdout=PIPE)
    assert mock_tool.parse_output(cp.stdout)[0]['args'][0] == '+nightly-2019-08-26'
