#!/usr/bin/env python3
"""Check the built CLI and MCP server using a disposable skhd config and data folder."""
import json
import os
from pathlib import Path
import selectors
import subprocess
import sys
import tempfile

binary = Path(sys.argv[1] if len(sys.argv) > 1 else 'target/debug/keyclash').resolve()


def main():
    with tempfile.TemporaryDirectory(prefix='keyclash-agent-check-') as folder:
        root = Path(folder)
        config = root / 'config' / 'skhd' / 'skhdrc'
        config.parent.mkdir(parents=True)
        original = '# disposable fixture\nctrl + alt - f18 : echo keyclash-agent-fixture\n'
        config.write_text(original)
        env = dict(os.environ, KEYCLASH_DATA_DIR=str(root / 'data'), XDG_CONFIG_HOME=str(root / 'config'))

        def cli(*args, success=True):
            run = subprocess.run([str(binary), *args], env=env, capture_output=True, text=True, timeout=45)
            assert (run.returncode == 0) == success, run.stderr
            return json.loads(run.stdout if success else run.stderr)

        report = cli('--report', '--json')
        assert report['schema_version'] == 1
        assert report['free_keys'] and report['all_shortcuts']
        assert len(cli('--tool', 'help')) == 10
        assert 'error' in cli('--tool', 'lookup', '{bad', success=False)
        assert 'error' in cli('--tool', 'lookup', '{"keys": false}', success=False)

        with (root / 'stderr.log').open('w') as errors:
            proc = subprocess.Popen([str(binary), '--mcp'], env=env, stdin=subprocess.PIPE,
                                    stdout=subprocess.PIPE, stderr=errors, text=True, bufsize=1)
            sequence = 0
            selector = selectors.DefaultSelector()
            selector.register(proc.stdout, selectors.EVENT_READ)

            def request(method, params=None):
                nonlocal sequence
                sequence += 1
                message = dict(jsonrpc='2.0', id=sequence, method=method)
                if params is not None:
                    message['params'] = params
                proc.stdin.write(json.dumps(message) + '\n')
                proc.stdin.flush()
                assert selector.select(45), f'Timed out waiting for {method}'
                response = json.loads(proc.stdout.readline())
                assert response['id'] == sequence, response
                return response

            def call(name, arguments=None, success=True):
                response = request('tools/call', dict(name=name, arguments=arguments or {}))
                assert 'result' in response, response
                result = response['result']
                assert result['isError'] != success, result
                return result['structuredContent'] if success else result['content'][0]['text']

            try:
                assert request('tools/list')['error']['code'] == -32000
                init = request('initialize', dict(protocolVersion='2025-11-25', capabilities={},
                                                 clientInfo=dict(name='keyclash-check', version='1')))
                assert init['result']['protocolVersion'] == '2025-11-25'
                proc.stdin.write('{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
                proc.stdin.flush()
                assert len(request('tools/list')['result']['tools']) == 10
                assert request('ping')['result'] == {}
                rows = call('list_shortcuts', dict(app='skhd'))['shortcuts']
                fixture = next((s for s in rows if s['detail'] == str(config)), None)
                assert fixture, 'Fixture not discovered; another configured skhd path may take precedence. No config was changed.'
                plan = call('propose_change', dict(id=fixture['id'], new_keys='ctrl opt f19'))
                assert config.read_text() == original, 'Proposal changed the config'
                assert '- ctrl + alt - f18' in plan['diff']
                assert '+ alt + ctrl - f19' in plan['diff']
                config.write_text(original + '# external edit\n')
                assert 'changed after' in call('apply_change', dict(plan_id=plan['plan_id']), success=False)
                config.write_text(original)
                done = call('apply_change', dict(plan_id=plan['plan_id']))
                changed = config.read_text()
                assert 'alt + ctrl - f19' in changed
                assert Path(done['backup']).read_text() == original
                assert done['new_id']
                assert 'changed after' in call('apply_change', dict(plan_id=plan['plan_id']), success=False)
                config.write_text(changed + '# preserve this\n')
                assert 'did not undo' in call('undo_last', success=False)
                assert config.read_text().endswith('# preserve this\n')
                config.write_text(changed)
                call('undo_last')
                assert config.read_text() == original
                assert call('list_shortcuts', dict(app='skhd', keys='ctrl opt f18'))['total'] >= 1
                assert 'no change to undo' in call('undo_last', success=False)
                # Proposals and history also survive independent CLI processes.
                plan = cli('--tool', 'propose_change', json.dumps(dict(id=fixture['id'], new_keys='ctrl opt f19')))
                cli('--tool', 'apply_change', json.dumps(dict(plan_id=plan['plan_id'])))
                assert 'alt + ctrl - f19' in config.read_text()
                cli('--tool', 'undo_last')
                assert config.read_text() == original
            finally:
                proc.stdin.close()
                try:
                    proc.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
                selector.close()
            assert proc.returncode == 0, (root / 'stderr.log').read_text()
        print('PASS: JSON report, CLI errors, MCP handshake, 10 tools, dry run, stale apply, backup, rescan, guarded undo, and restoration.')
        print('Writes used only the disposable skhd fixture and isolated Keyclash data.')


if __name__ == '__main__':
    main()
