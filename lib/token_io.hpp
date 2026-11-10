
inline std::string read_token(Loc loc) {
    while (ascii_space(rusk_peek())) rusk_get();
    if (rusk_peek() == std::char_traits<char>::eof()) fail(loc, "unexpected end of input");
    std::string token;
    while (rusk_peek() != std::char_traits<char>::eof() && !ascii_space(rusk_peek())) token.push_back(static_cast<char>(rusk_get()));
    return token;
}
inline void require_ascii(const std::string& text, Loc loc) {
    for (unsigned char c : text) if (c > 127) fail(loc, "non-ASCII input");
}


