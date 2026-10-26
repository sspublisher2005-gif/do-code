#!/usr/bin/env python3
"""Extra read-only/fail-closed acceptance probes against a compiled Forge CLI binary."""
from __future__ import annotations
import argparse, json, os, platform, subprocess, tempfile, time
from pathlib import Path

def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--binary', required=True)
    ap.add_argument('--report', required=True)
    args = ap.parse_args()
    binary = Path(args.binary).resolve()
    tests = []
    def probe(name, fn):
        start = time.perf_counter()
        try:
            detail = fn()
            tests.append({'name': name, 'status': 'PASS', 'duration_ms': round((time.perf_counter()-start)*1000, 3), 'detail': detail or ''})
        except Exception as e:
            tests.append({'name': name, 'status': 'FAIL', 'duration_ms': round((time.perf_counter()-start)*1000, 3), 'detail': str(e)})
    def run(args, cwd=None):
        return subprocess.run([str(binary), *args], cwd=cwd, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=10)
    def version():
        r=run(['--version']); assert r.returncode==0 and 'forge 0.7.0-rs.1' in r.stdout, (r.returncode,r.stdout); return r.stdout.strip()
    def help_policy():
        r=run(['--help']); assert r.returncode==0, r.stdout
        for s in ('Forge CLI', 'sandbox', 'memory', 'mission', 'apply', 'verify', 'undo'):
            assert s in r.stdout, f'missing help surface: {s}'
        assert '--max-cost-usd' not in r.stdout and '--estimated-cost' not in r.stdout
        return 'Expected core commands present; predicted-cost flags absent'
    def sandbox_status():
        r=run(['sandbox','status']); assert r.returncode==0, r.stdout
        assert 'NOT IMPLEMENTED' in r.stdout and 'execution is disabled' in r.stdout, r.stdout
        return 'Status reports no sandbox and keeps arbitrary execution disabled'
    def sandbox_fails_closed():
        r=run(['sandbox','self-test']); assert r.returncode != 0, r.stdout
        assert 'no native sandbox driver is implemented' in r.stdout.lower(), r.stdout
        assert 'No command was executed.' in r.stdout, r.stdout
        return 'Nonzero status and explicit no-execution message; no user command executed'
    def empty_mission_list():
        with tempfile.TemporaryDirectory(prefix='forge-extra-probe-') as tmp:
            r=run(['--root',tmp,'mission','list']); assert r.returncode==0, r.stdout
            assert '0 mission(s)' in r.stdout, r.stdout
            return 'Fresh empty temporary workspace returns 0 missions'
    def subcommand_help():
        names=['memory','pilot','decisions','evidence','resume','apply','verify','freeze','unfreeze','index','context','agents']
        for name in names:
            r=run([name,'--help'])
            assert r.returncode==0, f'{name}: {r.returncode}: {r.stdout}'
        return f'{len(names)} subcommand help surfaces exit 0'
    probe('version probe', version)
    probe('help surface and no predicted cost flags', help_policy)
    probe('sandbox status discloses no execution driver', sandbox_status)
    probe('sandbox self-test fails closed without command execution', sandbox_fails_closed)
    probe('mission list on clean temporary root', empty_mission_list)
    probe('secondary command help surfaces', subcommand_help)
    report={'product':'Forge CLI 0.7.0-rs.1','kind':'additional_binary_safety_probes','environment':{'os':platform.platform(),'python':platform.python_version(),'binary':str(binary)},'summary':{'passed':sum(x['status']=='PASS' for x in tests),'failed':sum(x['status']=='FAIL' for x in tests),'tests':len(tests)},'network_calls_intended':0,'real_model_calls':0,'tests':tests,'limitations':['Probes validate CLI disclosures, no-cost-policy surface and fail-closed behavior; they do not prove a real OS sandbox exists.','No Rust source compilation occurred.']}
    Path(args.report).write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    for t in tests: print(f"{t['status']} {t['name']}: {t['detail']}")
    print(f"SUMMARY {report['summary']['passed']} PASS, {report['summary']['failed']} FAIL")
    return 1 if report['summary']['failed'] else 0
if __name__=='__main__': raise SystemExit(main())
