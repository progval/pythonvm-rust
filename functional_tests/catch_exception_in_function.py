class Foo(BaseException):
    pass
class Bar(BaseException):
    pass

def raise_foo():
    raise Foo()

def raise_bar():
    raise Bar()

try:
    raise_foo()
except:
    print('raised')
else:
    print('not raised')

print('----')

try:
    raise_foo()
except Foo:
    print('raised Foo')
else:
    print('not raised')

print('----')

try:
    raise_foo()
except Foo:
    print('raised Foo')
except Bar:
    print('raised Bar')
else:
    print('not raised')

print('----')

try:
    raise_bar()
except Foo:
    print('raised Foo')
except Bar:
    print('raised Bar')
else:
    print('not raised')

print('----')

