class Foo:
    bar = 5
    print(bar)

print('---')

print(Foo.bar)

f = Foo()
print(f.bar)

f.bar = 6
print(Foo.bar)
print(f.bar)

Foo.bar = 7
print(Foo.bar)
print(f.bar)
