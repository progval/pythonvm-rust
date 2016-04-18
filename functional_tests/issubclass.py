class Foo:
    pass

class Bar(Foo):
    pass

class Qux(Foo):
    pass

print('issubclass(Foo, Foo) = ')
print(issubclass(Foo, Foo))
print('issubclass(Bar, Foo) = ')
print(issubclass(Bar, Foo))
print('issubclass(Foo, Bar) = ')
print(issubclass(Foo, Bar))
print('issubclass(Bar, Bar) = ')
print(issubclass(Bar, Bar))
print('issubclass(Qux, Qux) = ')
print(issubclass(Qux, Qux))
print('issubclass(Qux, Foo) = ')
print(issubclass(Qux, Foo))
print('issubclass(Qux, Bar) = ')
print(issubclass(Qux, Bar))
