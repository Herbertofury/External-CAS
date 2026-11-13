# Installed source localization boundary

`native/fidelity/src/localization.rs` reads STBL version 5 from one explicitly selected locale/provider package. Every live table ordinal, original entry order, raw UTF-8 bytes, flags, reserved bytes, declared length, trailing bytes and provider/index/payload pin is retained. Conflicting hash values refuse lookup; duplicates with identical text remain source-backed. Invalid UTF-8 and unknown flags stay explicit.

Parsing has byte budgets, checked counts/ranges and cancellation between resource reads. It performs no game, profile or package writes. The caller loads it on a worker and supplies the immutable snapshot to its UI host.

The first qualified native LocKey subset returns exact installed literal strings with no tokens, grammar directives or flags. Sim/object/date/list tokens, grammar/variant processing and filename-safe flags remain unsupported. They must not be replaced with placeholder text or fabricated Sim data.

Qualification: all 58,419 entries of one installed English table matched unchanged independent S4TK 0.6.14 by hash, text and order. Its 4,307,230-byte payload also matched the independent legacy C# RefPack decoder used to qualify the asset reader. Private receipts retain source pins and strings outside Git. Tests cover every truncated prefix, excessive counts, duplicate conflicts, unknown bytes/flags, invalid UTF-8 and unsupported formatting.

The isolated original AS3 navigation movie invoked its original static LocKey Function, through a native host binding, and received the exact installed label. This proves that bridge subset. Full source UI lifecycle, global library initialization, host services and Scaleform compatibility remain separate acceptance gates.
