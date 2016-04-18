class Foo:
    pass

class Bar(Foo):
    pass

class Qux(Foo):
    pass

print('isinstance(Foo(), Foo) =')
print(isinstance(Foo(), Foo))
print('isinstance(Bar(), Foo) =')
print(isinstance(Bar(), Foo))
print('isinstance(Foo(), Bar) =')
print(isinstance(Foo(), Bar))
print('isinstance(Bar(), Bar) =')
print(isinstance(Bar(), Bar))
print('isinstance(Qux(), Qux) =')
print(isinstance(Qux(), Qux))
print('isinstance(Qux(), Foo) =')
print(isinstance(Qux(), Foo))
print('isinstance(Qux(), Bar) =')
print(isinstance(Qux(), Bar))
