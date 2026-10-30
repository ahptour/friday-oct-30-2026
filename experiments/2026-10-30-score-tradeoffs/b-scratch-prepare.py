from pathlib import Path
import subprocess
root=Path.cwd();base=subprocess.check_output(['git','show','e2db239:track-b/src/main.rs']).decode()
phrase=base.replace('phrase: Option<usize>', 'phrase: usize').replace('q.phrase.is_none()', 'q.phrase == usize::MAX').replace('q.phrase.is_some()', 'q.phrase != usize::MAX').replace('phrase: None', 'phrase: usize::MAX').replace('query.phrase = Some(phrases)', 'query.phrase = phrases')
phrase=phrase.replace('    let mut matched = vec![u32::MAX; queries.len()];', '    let phrases = queries.iter().filter(|q| q.phrase != usize::MAX).count();\n    let mut matched = vec![u32::MAX; phrases];')
phrase=phrase.replace('            let terms = &queries[id].terms;', '            let query = &queries[id];\n            let terms = &query.terms;').replace('matched[id]', 'matched[query.phrase]')
both=phrase.replace('struct Clause {\n    query: usize,', 'struct Clause {\n    query: usize,\n    mask: usize,')
both=both.replace('    for (id, query) in queries\n        .iter()\n        .enumerate()\n        .filter(|(_, q)| q.phrase == usize::MAX)', '    for (mask, (id, query)) in queries\n        .iter()\n        .enumerate()\n        .filter(|(_, q)| q.phrase == usize::MAX)\n        .enumerate()')
both=both.replace('                query: id,\n                anchor,', '                query: id,\n                mask,\n                anchor,')
both=both.replace('vec![0u64; queries.len()]', 'vec![0u64; queries.len() - phrases]')
both=both.replace('query_bits[clause.query]', 'query_bits[clause.mask]').replace('touched.push(clause.query)', 'touched.push(id)')
both=both.replace('    for &query in touched.iter() {\n        let word = query_bits[query];\n        query_bits[query] = 0;', '    for &id in touched.iter() {\n        let clause = &clauses[id];\n        let query = clause.query;\n        let word = query_bits[clause.mask];\n        query_bits[clause.mask] = 0;')
for variant,source in [('base',base),('phrase',phrase),('both',both)]:
 context=root/f'.out/b-next/{variant}';(context/'src').mkdir(parents=True,exist_ok=True)
 for filename in ['Cargo.toml','Cargo.lock','Dockerfile']:(context/filename).write_bytes(subprocess.check_output(['git','show',f'e2db239:track-b/{filename}']))
 (context/'src/main.rs').write_text(source)
