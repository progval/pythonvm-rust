class Foo(BaseException):
    pass
class Bar(BaseException):
    pass

try:
    raise Foo()
except:
    print('raised')
else:
    print('not raised')

print('----')

try:
    pass
except:
    print('raised')
else:
    print('not raised')

print('----')

try:
    raise Foo()
except Foo:
    print('raised Foo')
else:
    print('not raised')

print('----')

try:
    raise Foo()
except Foo:
    print('raised Foo')
except Bar:
    print('raised Bar')
else:
    print('not raised')

print('----')

try:
    raise Bar()
except Foo:
    print('raised Foo')
except Bar:
    print('raised Bar')
else:
    print('not raised')

print('----')

try:
    try:
        raise Bar()
    except Foo:
        print('raised Foo')
except Bar:
    print('raised Bar')
else:
    print('not raised')

print('----')
