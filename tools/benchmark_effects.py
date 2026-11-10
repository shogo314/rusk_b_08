#!/usr/bin/env python3
"""Compare effect-driven C++ representation changes on identical Rusk sources.

Compilation and instrumentation are excluded from timings. Optional reference
copy counts hook libstdc++'s shared_ptr increment in preprocessed GCC headers;
these counts are a development measurement, not a portable runtime interface.
"""
import argparse
import datetime
import hashlib
import json
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
CASES = {
    'dictionary_copy_assignment': ('let d:=Dict::<Int,Vec<Int>>();for i in 0..500{d.set(i,[i,i+1]);}'
        'let target:=Dict::<Int,Vec<Int>>();let total:=0;for round in 0..100{let duplicate=d;'
        'target=duplicate;total+=target.get(round%500)?![0];}wt total,target.len();'),
    'set_copy_assignment': ('let s:=Set::<(Int,Int)>();for i in 0..1000{s.insert((i,i+1));}'
        'let target:=Set::<(Int,Int)>();let total:=0;for round in 0..100{let duplicate=s;'
        'target=duplicate;if target.contains((round,round+1)){total+=1;}}wt total,target.len();'),
    'dictionary_iteration': ('let d:=Dict::<Int,Int>();for i in 0..1000{d.set(i,i*3);}'
        'let total:=0;for round in 0..200{for entry in d{total+=entry[0]+entry[1];}}wt total;'),
    'set_iterator_copy': ('let s:=Set::<Int>();for i in 0..1000{s.insert(i);}'
        'let total:=0;for round in 0..200{let it:=s.filter((x:Int)=>x%2==0).map((x:Int)=>x+1);'
        'let duplicate=it;total+=it.fold(0,(a:Int,b:Int)=>a+b);'
        'total+=duplicate.fold(0,(a:Int,b:Int)=>a+b);}wt total;'),
    'dictionary_operations': ('let d:=Dict::<Int,Int>();let total:=0;'
        'for round in 0..100000{let key:=round%1000;d.set(key,round);'
        'total+=d[key];if d.contains(key){total+=1;}total+=d.get(key)?!;'
        'if round%3==0{d.erase(key);}}wt total,d.len();'),
    'set_operations': ('let s:=Set::<Int>();let total:=0;'
        'for round in 0..100000{let key:=round%1000;if s.insert(key){total+=1;}'
        'if s.contains(key){total+=1;}total+=s.get(key)??0;'
        'if round%3==0{s.erase(key);}}wt total,s.len();'),
    'dictionary_defaults_exports': ('let d:=Dict::<Int,Vec<Int>>();'
        'for i in 0..1000{d[i].push_back(i);}let total:=0;'
        'for round in 0..100{let keys:=d.keys();let values:=d.values();'
        'total+=keys.len()+values.len();for value in values{total+=value[0];}}wt total;'),

    'opaque_record_assignment': ('extern type Leaf{cpp_name:"std::int64_t",}'
        'extern fn leaf()->Leaf{cpp_name:"assignment_leaf",code:"Obj<std::int64_t> assignment_leaf(Loc){return make_obj<std::int64_t>(1);}",}'
        'extern fn copy(x:Leaf)->Leaf{cpp_name:"assignment_leaf_copy",code:"Obj<std::int64_t> assignment_leaf_copy(Obj<std::int64_t> x,Loc){return make_obj<std::int64_t>(*x);}",}'
        'struct Row{value:Leaf;}let a:=Row{value:leaf()};let b:=Row{value:leaf()};'
        'for round in 0..100000{a=b;}wt 1;'),
    'record_assignment': ('struct Row{value:Int;text:Str;}let a:=Row{value:0,text:""};'
                           'let b:=Row{value:1,text:"ab"};let total:=0;'
                           'for round in 0..100000{a=b;total+=a.value;}wt total;'),
    'enum_assignment': ('enum Item{Pair(Int,Str),Empty}let a:=Item::Empty;'
                         'let b:=Item::Pair(1,"ab");let total:=0;for round in 0..100000{a=b;'
                         'match a{Pair(n,s)=>{total+=n;},Empty=>{},};}wt total;'),
    'option_vector_copy': ('let a:=[Some(1);1000];let total:=0;'
                           'for round in 0..100{let b=a;total+=b[0].value();}wt total;'),
    'enum_vector_copy': ('enum Item{Pair(Int,Str),Empty}let a:=[Item::Pair(1,"ab");1000];'
                         'let total:=0;for round in 0..100{let b=a;match b[0]{Pair(n,s)=>{total+=n;},Empty=>{},};}wt total;'),
    'option_input': ('let total:=0;while let Some(value)=try_read::<Int>(){total+=value;}wt total;'),
    'option_construction': ('let total:=0;for i in 0..200000{let option:=Some(i);'
                            'match option{Some(value)=>{total+=value;},None=>{},};}wt total;'),
    'option_operations': ('let total:=0;for i in 0..100000{let option:=Some(i);'
                          'if option.is_some(){total+=option.value();}'
                          'let empty:=Option::<Int>();if empty.is_none(){total+=1;}'
                          'if !empty{total+=1;}}wt total;'),
    'record_vector_copy': ('struct Row{value:Int;text:Str;}let a:=[Row{value:1,text:"ab"};1000];'
                           'let total:=0;for round in 0..100{let b=a;total+=b[0].value;}wt total;'),
    'vector_copy': ('let a:=[[1,2,3,4];500];let total:=0;for round in 0..200{'
                    'let b=a;total+=b[0][0];a=b;}wt total;'),
    'vector_capacity_reuse': ('let a:=[1;10000];let small:=[2;4];let total:=0;'
                             'for round in 0..100{a=small;for i in 0..10000{a.push_back(i);}'
                             'total+=a.len();}wt total;'),
    'tuple_comparisons': ('let a:=([1,2,3],(4,"x"));let b:=([1,2,3],(4,"y"));'
                           'let total:=0;for round in 0..200000{'
                           'if a==b{total+=1;}if a!=b{total+=1;}if a<b{total+=1;}'
                           'if a<=b{total+=1;}if a>b{total+=1;}if a>=b{total+=1;}}wt total;'),
    'tuple_output': ('let row:=((1,2),[3,4],true,"ok");for round in 0..100000{wt row;}'),
    'vector_output': ('let values:=[123;1000];let matrix:=[values,values];'
                      'let rows:=[(1,2);1000];for round in 0..500{wt matrix,rows;}'),
    'dimensions_input': ('in cube(100,30,100):Vec<Vec<Vec<Int>>>;'
                         'let total:=0;for round in 0..20000{in row(2,2):Vec<Vec<Int>>;total+=row[1][1];}'
                         'wt cube.len(),cube[-1][-1][-1],total;'),
    'vector_input': ('in values(300000):Vec<Int>,rows(100000):Vec<(Int,Int)>;'
                     'wt values.len(),values[-1],rows.len(),rows[-1][1];'),
    'vector_ordering': ('let a:=[0;4000];let total:=0;for round in 0..60{'
                        'for i in 0..a.len(){a[i]=(i*7919+round)%4001;}'
                        'let ordered:=sorted(a);sort(a);total+=a[2000]+ordered[2000];}wt total;'),
    'vector_search': ('let a:=[0;2000];for i in 0..a.len(){a[i]=i/3;}let total:=0;'
                      'for i in 0..20000{let key:=i%700;total+=bisect_left(a,key);'
                      'total+=bisect_right(a,key);if contains(a,key){total+=1;}}wt total;'),
    'vector_comparisons': ('let a:=[1;2000];let b:=[1;2000];let total:=0;'
                           'for round in 0..20000{b[-1]=round%3;'
                           'if a==b{total+=1;}if a<b{total+=1;}}wt total;'),
    'extern_key_sort': ('let a:=[0;2000];let total:=0;for round in 0..200{'
                       'for i in 0..a.len(){a[i]=(i*7919+round)%2003;}'
                       'sort_by_key(a,(x:Int)=>x%97);total+=a[1000];}wt total;'),
    'extern_aggregate_key_sort': ('let a:=[0;1000];let total:=0;for round in 0..200{'
                                 'for i in 0..a.len(){a[i]=(i*7919+round)%1009;}'
                                 'sort_by_key(a,(x:Int)=>(x%97,-x));total+=a[500];'
                                 'sort_by_key(a,(x:Int)=>[x%97,-x]);total+=a[500];}wt total;'),
    'extern_comparator_sort': ('let a:=[0;2000];let total:=0;for round in 0..30{'
                               'for i in 0..a.len(){a[i]=(i*7919+round)%2003;}'
                               'sort_by_comp(a,(x:Int,y:Int)=>x<y);total+=a[1000];}wt total;'),
    'extern_predicates': ('let visited:=0;let a:=[1;10000];let total:=0;for i in 0..200{'
                          'if all(a,(x:Int)=>{visited+=1;x==1}){total+=1;}'
                          'if any(a,(x:Int)=>x==1){total+=1;}}wt total,visited;'),
    'extern_reductions': ('let a:=[1;10000];let total:=0;for i in 0..50{'
                          'total+=fold(a,0,(x:Int,y:Int)=>x+y);'
                          'let prefix:=scan(a,0,(x:Int,y:Int)=>x+y);total+=prefix.len();'
                          'total+=to_vec(a).len();}wt total;'),
    'extern_first_true': ('let total:=0;for i in 0..100000{let threshold:=i%1000000;'
                          'total+=first_true(0,1000000,(x:Int)=>x>=threshold);}wt total;'),
    'scalar_call': ('fn add_one(x:Int)->Int{return x+1;}let i:=0;let total:=0;'
                    'while i<500000{total+=add_one(i);i+=1;}wt total;'),
    'element_call': ('fn add_one(x:Int)->Int{return x+1;}'
                     'let a:=[1;100000];let total:=0;'
                     'for i in 0..20{for x in a{total+=add_one(x);}}wt total;'),
    'find': 'let a:=[1;100000];let total:=0;for i in 0..20{total+=find(a,2)??-1;}wt total;',
    'sum_native': 'let a:=[1;1000000];let total:=0;for i in 0..20{total+=sum(a);}wt total;',
    'sum_rusk': ('fn rusk_total(a:Vec<Int>)->Int{let total:=0;for x in a{total+=x;}return total;}'
                 'let a:=[1;1000000];let total:=0;for i in 0..20{total+=rusk_total(a);}wt total;'),
}
INPUTS = {'dimensions_input': b'1 '*380000, 'vector_input': b'1 '*500000, 'option_input': b'1 '*200000}
ALLOCATION_COUNTER = r'''
#include <new>
#include <cstdio>
static unsigned long long measured_allocations=0;
void* operator new(std::size_t size){++measured_allocations;if(auto p=std::malloc(size?size:1))return p;throw std::bad_alloc();}
void operator delete(void* p)noexcept{std::free(p);}
void operator delete(void* p,std::size_t)noexcept{std::free(p);}
struct AllocationReport{~AllocationReport(){std::fprintf(stderr,"allocations=%llu\n",measured_allocations);}} allocation_report;
'''

def checked(command, input_data=None):
    run = subprocess.run(command, capture_output=True, input=input_data)
    if run.returncode:
        raise RuntimeError(f'{command}:\n{run.stderr.decode(errors="replace")}')
    return run


def build(source, executable):
    checked(['g++', '-std=c++20', '-O2', str(source), '-o', str(executable)])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler', type=Path, default=ROOT/'target/debug/rusk')
    parser.add_argument('--baseline-compiler', type=Path, required=True)
    parser.add_argument('--repeats', type=int, default=7)
    parser.add_argument('--reference-copies', action='store_true')
    parser.add_argument('--case', action='append', choices=CASES)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error('--repeats must be positive')
    compilers = {'baseline': args.baseline_compiler.resolve(), 'current': args.compiler.resolve()}
    report = {
        'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'platform': platform.platform(),
        'cpu': next((line.split(':', 1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines() if line.startswith('model name')), platform.processor()),
        'gcc': checked(['g++', '--version']).stdout.decode().splitlines()[0],
        'compiler_sha256': {name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in compilers.items()},
        'repeats': args.repeats, 'flags': ['-std=c++20', '-O2'],
        'timing': 'interleaved; compilation excluded; instrumentation measured separately', 'cases': {},
    }
    with tempfile.TemporaryDirectory(prefix='rusk-effect-bench-') as directory:
        directory = Path(directory)
        jobs = []
        for name in args.case or CASES:
            source = directory/(name+'.rusk')
            source.write_text(CASES[name])
            input_data = INPUTS.get(name)
            row = {'source': CASES[name], 'variants': {}}
            if input_data is not None:
                row['input_bytes'] = len(input_data)
                row['input_sha256'] = hashlib.sha256(input_data).hexdigest()
            report['cases'][name] = row
            expected = None
            for variant, compiler in compilers.items():
                stem = directory/(name+'-'+variant)
                cpp = stem.with_suffix('.cpp')
                checked([str(compiler), 'compile', str(source), '-o', str(cpp)])
                build(cpp, stem)
                output = checked([str(stem)], input_data).stdout
                if expected is None:
                    expected = output
                assert output == expected, (name, variant, output, expected)
                data = {'cpp_sha256': hashlib.sha256(cpp.read_bytes()).hexdigest(), 'runs_ms': []}
                if len(output) <= 4096:
                    data['output'] = output.decode()
                else:
                    data['output_bytes'] = len(output)
                    data['output_sha256'] = hashlib.sha256(output).hexdigest()
                row['variants'][variant] = data
                instrumented = cpp.read_text().replace('#include <limits>', '#include <limits>\n'+ALLOCATION_COUNTER, 1)
                counted = directory/(name+'-'+variant+'-alloc.cpp')
                counted.write_text(instrumented)
                executable = counted.with_suffix('')
                build(counted, executable)
                run = checked([str(executable)], input_data)
                assert run.stdout == expected
                data['allocations'] = int(run.stderr.decode().strip().split('=')[1])
                if args.reference_copies:
                    text = checked(['g++', '-std=c++20', '-E', str(cpp)]).stdout.decode()
                    hooks = [
                        ('_M_add_ref_copy()\n      { __gnu_cxx::__atomic_add_dispatch(&_M_use_count, 1); }',
                         '_M_add_ref_copy()\n      { ++::measured_refs; __gnu_cxx::__atomic_add_dispatch(&_M_use_count, 1); }'),
                        ('_Sp_counted_base<_S_single>::_M_add_ref_copy()\n    { ++_M_use_count; }',
                         '_Sp_counted_base<_S_single>::_M_add_ref_copy()\n    { ++::measured_refs; ++_M_use_count; }'),
                    ]
                    for before, after in hooks:
                        if text.count(before) != 1:
                            raise RuntimeError('unsupported libstdc++ shared_ptr layout; omit --reference-copies')
                        text = text.replace(before, after)
                    text = 'unsigned long long measured_refs=0;\n'+text
                    text += '\nstruct RefReport{~RefReport(){std::fprintf(stderr,"copies=%llu\\n",measured_refs);}} ref_report;\n'
                    counted = directory/(name+'-'+variant+'-refs.cpp')
                    counted.write_text(text)
                    executable = counted.with_suffix('')
                    build(counted, executable)
                    run = checked([str(executable)], input_data)
                    assert run.stdout == expected
                    data['shared_ptr_copies'] = int(run.stderr.decode().strip().split('=')[1])
                jobs.append((name, variant, stem, data, expected, input_data))
            print(name, 'built and checked', flush=True)
        # Start timings only after all builds/counters complete. Reverse order
        # every other round, preserving every sample in the result artifact.
        for repeat in range(args.repeats):
            for name, variant, executable, data, expected, input_data in jobs[::1 if repeat % 2 == 0 else -1]:
                start = time.perf_counter()
                run = checked([str(executable)], input_data)
                elapsed = (time.perf_counter()-start)*1000
                assert run.stdout == expected and not run.stderr, (name, variant)
                data['runs_ms'].append(round(elapsed, 6))
        for name, row in report['cases'].items():
            for data in row['variants'].values():
                data['median_ms'] = statistics.median(data['runs_ms'])
            row['speedup'] = row['variants']['baseline']['median_ms']/row['variants']['current']['median_ms']
            print(name, row, flush=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n')


if __name__ == '__main__':
    main()
