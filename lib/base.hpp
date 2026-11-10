#include <cstdint>
#include <cstdlib>
#include <iostream>
#include <memory>
#include <utility>
#include <string>
#include <charconv>
#include <limits>
#include <system_error>
struct Unit {};
struct Loc { const char* file; unsigned line, column; };
template<class T> using Obj = std::shared_ptr<T>;
template<class T> Obj<T> make_obj(T value) { return std::make_shared<T>(std::move(value)); }
[[noreturn]] inline void fail(Loc loc, const char* message) {
    std::cerr << loc.file << ':' << loc.line << ':' << loc.column << ": " << message << '\n';
    std::exit(1);
}
inline std::int64_t checked_int(__int128 value, Loc loc) {
    if (value < std::numeric_limits<std::int64_t>::min() || value > std::numeric_limits<std::int64_t>::max()) fail(loc, "integer overflow");
    return static_cast<std::int64_t>(value);
}
inline bool ascii_space(int c) { return c == ' ' || (c >= 9 && c <= 13); }
