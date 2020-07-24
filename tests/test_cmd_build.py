"""Tests `oasis build`."""

import os
import os.path as osp
import shutil
from subprocess import PIPE


def test_build_multiproj(oenv, temp_dir):
    multiproj_dir = osp.join(temp_dir, 'multiproj')
    target_dir = osp.join(multiproj_dir, 'target', 'service')

    shutil.copytree(osp.join(osp.dirname(__file__), 'res', 'multiproj'), multiproj_dir)

    # test no repo
    cp = oenv.run('oasis build', cwd=multiproj_dir, check=False, stderr=PIPE)
    assert 'could not find workspace' in cp.stderr

    oenv.run('git init .', cwd=multiproj_dir)

    # test not found
    cp = oenv.run('oasis build e', cwd=multiproj_dir, check=False, stderr=PIPE)
    assert 'no target named `e` found' in cp.stderr

    cp = oenv.run('oasis build ./asdf', cwd=multiproj_dir, check=False, stderr=PIPE)
    assert '`./asdf` does not refer to a target nor a directory' in cp.stderr

    cp = oenv.run('oasis build /', cwd=multiproj_dir, check=False, stderr=PIPE)
    assert 'the path `/` exists outside of this workspace' in cp.stderr

    # test build success
    oenv.run('oasis build', cwd=multiproj_dir)
    for svc in ['a', 'b', 'c', 'd']:
        assert osp.isfile(osp.join(target_dir, f'{svc}.wasm'))

    shutil.rmtree(target_dir)

    oenv.run('oasis build d', cwd=multiproj_dir)
    assert osp.isfile(osp.join(target_dir, 'd.wasm'))
    for svc in ['a', 'b', 'c']:
        assert not osp.isfile(osp.join(target_dir, f'{svc}.wasm'))

    shutil.rmtree(target_dir)

    # test cwd-is-not-root builds
    workspace_dir = osp.join(multiproj_dir, 'random', 'dir', 'here')
    os.makedirs(workspace_dir, exist_ok=True)

    oenv.run('oasis build b', cwd=workspace_dir)
    assert osp.isfile(osp.join(target_dir, 'b.wasm'))
    assert osp.isfile(osp.join(target_dir, 'c.wasm'))

    shutil.rmtree(target_dir)

    oenv.run('oasis build :/', cwd=workspace_dir)
    for svc in ['a', 'b', 'c', 'd']:
        assert osp.isfile(osp.join(target_dir, f'{svc}.wasm'))
