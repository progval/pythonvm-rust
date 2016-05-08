def f(foo, bar, *baz, qux, quux, fiz='buz', **kwargs):
    print('foo:', foo)
    print('bar:', bar)
    for x in baz:
        print('baz:', x)
    print('qux:', qux)
    print('quux:', quux)
    print('fiz:', fiz)
    # TODO: print kwargs
    print('--')

f(1, 2, qux=3, quux=4)
f(1, 2, 3, qux=4, quux=5)
f(1, 2, 3, 21, qux=4, quux=5)
f(1, 2, qux=3, quux=4, fiz=5)
f(1, 2, 3, qux=4, quux=5, fiz=6)
f(1, 2, 3, 21, qux=4, quux=5, fiz=6)
