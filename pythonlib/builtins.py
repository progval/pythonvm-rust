def print(value):
    __primitives__.write_stdout(value)
    __primitives__.write_stdout("\n")

__build__class__ = __primitives__.build_class
issubclass = __primitives__.issubclass
isinstance = __primitives__.isinstance
