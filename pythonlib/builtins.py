BaseException = __primitives__.BaseException
RuntimeError = __primitives__.RuntimeError

def print(*values, sep=' ', end='\n'):
    first = True
    for value in values:
        if first:
            first = False
        else:
            __primitives__.write_stdout(sep)
        __primitives__.write_stdout(value)

    __primitives__.write_stdout(end)

__build__class__ = __primitives__.build_class
issubclass = __primitives__.issubclass
isinstance = __primitives__.isinstance

