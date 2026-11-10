#include <vector>
#include <algorithm>
#include <unordered_map>
#include <sstream>
#include <iomanip>
template<class T> struct RuskVec : std::vector<Obj<T>> {
    using std::vector<Obj<T>>::vector;
    std::uint64_t version = 0;
};
using IntVec = RuskVec<std::int64_t>;
inline std::size_t checked_index(std::size_t size, std::int64_t index, Loc loc) {
    __int128 position = index;
    if (position < 0) position += static_cast<__int128>(size);
    if (position < 0 || position >= static_cast<__int128>(size)) fail(loc, "array index out of range");
    return static_cast<std::size_t>(position);
}
inline std::size_t vec_int_index(const IntVec& a, std::int64_t i, Loc loc) { return checked_index(a.size(), i, loc); }
template<class T> Obj<T> deep_copy(const Obj<T>& x, [[maybe_unused]] Loc loc = Loc{"<copy>",0,0}) { return make_obj<T>(*x); }
template<class T> Obj<RuskVec<T>> deep_copy(const Obj<RuskVec<T>>& x, Loc loc = Loc{"<copy>",0,0}) {
    auto result = make_obj<RuskVec<T>>(RuskVec<T>{});
    result->reserve(x->size());
    for (const auto& element : *x) result->push_back(deep_copy(element,loc));
    return result;
}
template<class T> struct RuskIter { Obj<RuskVec<T>> owner; std::size_t index; std::uint64_t version; };
template<class T> void check_iter(const Obj<RuskIter<T>>& it, Loc loc) {
    if(it->owner->version != it->version) fail(loc,"container changed during iteration");
}
struct RuskRange { std::int64_t start, end; };
struct RuskRangeIter { std::int64_t next, end; };
inline std::unordered_map<const std::string*, std::size_t> string_borrows;
inline void check_string_resize(const Obj<std::string>& s, Loc loc) {
    if (string_borrows.contains(s.get())) fail(loc,"string has active character references");
}
inline Obj<char> string_get(const Obj<std::string>& s, std::int64_t i, Loc loc) {
    auto position=checked_index(s->size(),i,loc);
    ++string_borrows[s.get()];
    return Obj<char>(&(*s)[position],[s](char*){ auto it=string_borrows.find(s.get()); if(--it->second==0) string_borrows.erase(it); });
}
inline char checked_char(__int128 value, Loc loc) {
    if(value<0 || value>127) fail(loc,"character out of ASCII range");
    return static_cast<char>(value);
}
template<class T> Obj<T> read_value(Loc loc);
template<> inline Obj<std::int64_t> read_value(Loc loc) {
    auto token=read_token(loc); const char* first=token.data(); const char* last=first+token.size();
    if(first!=last && *first=='+') { ++first; if(first==last || *first<'0'||*first>'9') fail(loc,"invalid integer input"); }
    std::int64_t n=0; auto result=std::from_chars(first,last,n);
    if(result.ec!=std::errc{}||result.ptr!=last) fail(loc,"invalid or out of range integer input");
    return make_obj<std::int64_t>(n);
}
template<> inline Obj<std::string> read_value(Loc loc) { auto s=read_token(loc); require_ascii(s,loc); return make_obj<std::string>(s); }
template<> inline Obj<char> read_value(Loc loc) {
    while(ascii_space(rusk_peek())) rusk_get(); int c=rusk_get();
    if(c<0||c>127) fail(loc,"invalid character input"); return make_obj<char>(static_cast<char>(c));
}
template<> inline Obj<bool> read_value(Loc loc) {
    auto s=read_token(loc); if(s=="Yes"||s=="true"||s=="1") return make_obj<bool>(true);
    if(s=="No"||s=="false"||s=="0") return make_obj<bool>(false); fail(loc,"invalid boolean input");
}
