#!/usr/bin/env python3
"""Location-indexed lexical QML audit, not a semantic QML parser.

SPDX-License-Identifier: MIT OR Apache-2.0
"""
import argparse
import ast
import collections
import json
import pathlib
import re
import subprocess


def mask(text, strings=False):
    pattern = r'//[^\n]*|/\*[\s\S]*?\*/'
    if strings:
        pattern = r'"(?:\\.|[^"\\])*"|\x27(?:\\.|[^\x27\\])*\x27|`(?:\\.|[^`\\])*`|' + pattern
    return re.sub(pattern, lambda m: re.sub(r'[^\n]', ' ', m.group()), text)


def blocks(text, typename):
    structural = mask(text, True)
    for match in re.finditer(r'\b' + re.escape(typename) + r'\s*\{', structural):
        depth, end = 1, match.end()
        while end < len(structural) and depth:
            depth += (structural[end] == '{') - (structural[end] == '}')
            end += 1
        yield match.start(), end, text[match.end():end - 1]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('reference', type=pathlib.Path)
    p.add_argument('output', type=pathlib.Path)
    p.add_argument('--revision', help='Exact revision for an exported read-only snapshot')
    args = p.parse_args()
    root = args.reference
    data = {'revision': args.revision or subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip(),
            'method': 'lexical imports/object declarations/member references; comment and string aware brace matching; dynamic expressions require runtime confirmation',
            'files': [], 'imports': {}, 'types': {}, 'ipc': [], 'processes': [], 'file_views': [], 'shortcuts': [], 'qualified_references': {}}
    data['all_process_file_mentions'] = {'Process': [], 'FileView': []}
    data['typed_properties'] = []
    imports, types, refs = collections.defaultdict(list), collections.defaultdict(list), collections.defaultdict(list)
    for file in sorted((root / 'nacre/shell').rglob('*.qml')):
        text = file.read_text()
        clean, structural = mask(text), mask(text, True)
        rel = file.relative_to(root).as_posix()
        def loc(offset):
            return f'{rel}:{text.count(chr(10), 0, offset) + 1}'
        fi = {'path': rel, 'lines': len(text.splitlines()), 'imports': [], 'types': []}
        for m in re.finditer(r'\bproperty\s+([\w.<>]+)\s+(\w+)\b',structural):
            data['typed_properties'].append({'type':m.group(1),'name':m.group(2),'location':loc(m.start())})
        for m in re.finditer(r'^\s*import\s+([^\n]+)', clean, re.M):
            imp = m.group(1).strip()
            imports[imp].append(loc(m.start()))
            fi['imports'].append(imp)
        for m in re.finditer(r'\b([A-Z]\w*(?:\.[A-Z]\w*)*)\s*\{', structural):
            # Source-audited false positive: /M{1,2}/ in NacreTime's JS regex.
            if m.group(1) == 'M' and rel.endswith('/NacreTime.qml'):
                continue
            types[m.group(1)].append(loc(m.start()))
            fi['types'].append(m.group(1))
        for m in re.finditer(r'\b([A-Z]\w*(?:\.\w+)+)', structural):
            refs[m.group(1)].append(loc(m.start()))
        for name in data['all_process_file_mentions']:
            data['all_process_file_mentions'][name].extend(loc(m.start()) for m in re.finditer(r'\b'+name+r'\b',structural))
        for name, key in [('IpcHandler', 'ipc'), ('Process', 'processes'), ('FileView', 'file_views'), ('GlobalShortcut', 'shortcuts'), ('NacreShortcut', 'shortcuts')]:
            for start, end, body in blocks(text, name):
                obj = {'type': name, 'location': loc(start)}
                if key == 'ipc':
                    target = re.search(r'\btarget\s*:\s*([^\n]+)', body)
                    obj['target'] = target.group(1).strip() if target else '(inherited/dynamic)'
                    obj['methods'] = [{'signature': m.group(1).strip(), 'location': loc(text.index(body, start) + m.start())}
                                      for m in re.finditer(r'\bfunction\s+(\w+\s*\([^)]*\)(?:\s*:\s*\w+)?)', mask(body))]
                else:
                    properties = {}
                    for line in mask(body).splitlines():
                        m = re.match(r'\s*(id|command|path|name|appId|running|watchChanges|onFileChanged|onExited|stdinEnabled|enabled|onPressed|onReleased)\s*:\s*(.*)', line)
                        if m:
                            properties.setdefault(m.group(1), []).append(m.group(2).strip())
                    obj['properties'] = properties
                    obj['block_end'] = loc(end)
                data[key].append(obj)
        data['files'].append(fi)
    data['imports'], data['types'], data['qualified_references'] = dict(sorted(imports.items())), dict(sorted(types.items())), dict(sorted(refs.items()))
    contract = json.loads((root / 'nacre/shell-tools/compositor-contract.json').read_text())
    data['compositor_contract'] = contract
    data['cli_catalog'] = json.loads((root / 'nacre/shell-tools/cli.json').read_text())
    calls = []
    for file in sorted((root / 'nacre/shell-tools').rglob('*')):
        if not file.is_file() or file.suffix not in {'.sh', '.py', '.json', '.lua', '.kdl', '.in'}:
            continue
        if 'tests' in file.parts:
            continue
        for number, line in enumerate(file.read_text(errors='replace').splitlines(), 1):
            if re.search(r'ipc[\s\x22,]+(?:call|\x22call)|\bqs.*\bipc|\bipc\(', line):
                calls.append({'location': f'{file.relative_to(root).as_posix()}:{number}', 'expression': line.strip()})
        if file.suffix == '.py':
            for node in ast.walk(ast.parse(file.read_text())):
                if isinstance(node, ast.Call) and any(isinstance(arg, ast.List) and any(isinstance(value, ast.Constant) and value.value == 'ipc' for value in arg.elts) for arg in node.args):
                    calls.append({'location': f'{file.relative_to(root).as_posix()}:{node.lineno}', 'expression': ast.unparse(node), 'method': 'Python AST multiline argv'})
    data['helper_cli_ipc_calls'] = calls
    data['helper_task_instances'] = []
    for file in sorted((root/'nacre/shell').rglob('*.qml')):
        text = file.read_text()
        for name in ['NacreHelperTask', 'NacrePaletteTask', 'NacreJsonTask']:
            for start, end, body in blocks(text, name):
                data['helper_task_instances'].append({'type': name, 'location': f'{file.relative_to(root).as_posix()}:{text.count(chr(10),0,start)+1}',
                    'command_lines': [line.strip() for line in body.splitlines() if re.search(r'\bcommand:',line)]})
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(data, indent=2, ensure_ascii=False) + '\n')
    print(json.dumps({'qml_files': len(data['files']), 'qml_lines': sum(f['lines'] for f in data['files']),
                      'imports': len(imports), 'object_types': len(types), **{k: len(data[k]) for k in ['ipc', 'processes', 'file_views', 'shortcuts', 'helper_cli_ipc_calls']}}, indent=2))


if __name__ == '__main__':
    main()
