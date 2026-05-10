" Vim syntax file for Cobrust (.cb)
" Language:   Cobrust
" Maintainer: Cobrust Project <https://github.com/cobrust-lang/cobrust>
" Version:    0.1.0
" License:    Apache-2.0 OR MIT
"
" Keywords sourced from:
"   crates/cobrust-frontend/src/token.rs  (match_keyword function)
"   crates/cobrust-frontend/src/lexer.rs  (lex_number, lex_string, lex_fstring)

if exists("b:current_syntax")
  finish
endif

" ── Comments ────────────────────────────────────────────────────────────────
syn match   cobrustComment      "#.*$" contains=cobrustTodo
syn keyword cobrustTodo         TODO FIXME HACK NOTE contained

" ── Keywords ────────────────────────────────────────────────────────────────
" Control flow (token.rs: KwIf, KwElif, KwElse, KwWhile, KwFor, KwMatch,
"               KwCase, KwReturn, KwBreak, KwContinue, KwPass, KwTry,
"               KwExcept, KwFinally, KwRaise, KwWith, KwYield, KwAwait)
syn keyword cobrustKeyword      if elif else while for match case
syn keyword cobrustKeyword      return break continue pass
syn keyword cobrustKeyword      try except finally raise
syn keyword cobrustKeyword      with yield await

" Declaration keywords (token.rs: KwFn, KwLet, KwClass, KwLambda, KwType)
syn keyword cobrustDefKeyword   fn let class lambda type

" Operator keywords (token.rs: KwAnd, KwOr, KwNot, KwIn, KwAs, KwFrom, KwImport)
syn keyword cobrustOpKeyword    and or not in as from import

" ── Literals ────────────────────────────────────────────────────────────────
" Boolean / None (token.rs: KwTrue, KwFalse, KwNone)
syn keyword cobrustBoolean      True False
syn keyword cobrustNone         None

" ── Types ───────────────────────────────────────────────────────────────────
" Primitive types (constitution §2.2 + token.rs design)
syn keyword cobrustType         i8 i16 i32 i64 i128
syn keyword cobrustType         u8 u16 u32 u64 u128
syn keyword cobrustType         f32 f64 bool str bytes isize usize

" Built-in generic types
syn keyword cobrustBuiltinType  List Dict Set Option Result Tuple

" ── Strings ─────────────────────────────────────────────────────────────────
" Double-quoted string
syn region  cobrustString       start=+"+  skip=+\\"+  end=+"+
                                \ contains=cobrustEscape
" Single-quoted string
syn region  cobrustString       start=+'+  skip=+\\'+  end=+'+
                                \ contains=cobrustEscape
" Triple-double-quoted string
syn region  cobrustString       start=+"""+  end=+"""+
                                \ contains=cobrustEscape
" Triple-single-quoted string
syn region  cobrustString       start=+'''+  end=+'''+
                                \ contains=cobrustEscape
" f-strings (prefix f or F, with or without r/R)
syn region  cobrustFString      start=+\v[fFrRbB]*[fF][rRbB]*"+  end=+"+
                                \ contains=cobrustEscape,cobrustFStringExpr
syn region  cobrustFString      start=+\v[fFrRbB]*[fF][rRbB]*'+  end=+'+
                                \ contains=cobrustEscape,cobrustFStringExpr
syn region  cobrustFString      start=+\v[fFrRbB]*[fF][rRbB]*"""+  end=+"""+
                                \ contains=cobrustEscape,cobrustFStringExpr
syn region  cobrustFString      start=+\v[fFrRbB]*[fF][rRbB]*'''+  end=+'''+
                                \ contains=cobrustEscape,cobrustFStringExpr
" f-string interpolation block {expr}
syn region  cobrustFStringExpr  start=+{+  end=+}+
                                \ contained contains=cobrustKeyword,cobrustBoolean,
                                \ cobrustNone,cobrustNumber,cobrustString
" Escape sequences (token.rs lex_string)
syn match   cobrustEscape       "\\[\\'"nrt0xuU]" contained

" ── Numbers ─────────────────────────────────────────────────────────────────
" Hex: 0xDEAD_BEEF
syn match   cobrustNumber       "\<0[xX][0-9A-Fa-f][0-9A-Fa-f_]*\>"
" Binary: 0b1010
syn match   cobrustNumber       "\<0[bB][01][01_]*\>"
" Octal: 0o755
syn match   cobrustNumber       "\<0[oO][0-7][0-7_]*\>"
" Float with exponent
syn match   cobrustNumber       "\<[0-9][0-9_]*\(\.[0-9][0-9_]*\)\?\([eE][+-]\?[0-9][0-9_]*\)\?[jJ]\?\>"
" Float starting with dot
syn match   cobrustNumber       "\.[0-9][0-9_]*\([eE][+-]\?[0-9][0-9_]*\)\?[jJ]\?"

" ── Decorators ──────────────────────────────────────────────────────────────
syn match   cobrustDecorator    "@[a-zA-Z_][a-zA-Z0-9_.]*"

" ── Function definitions and calls ─────────────────────────────────────────
syn match   cobrustFuncDef      "\<fn\s\+\zs[a-zA-Z_][a-zA-Z0-9_]*"
syn match   cobrustFuncCall     "\<[a-zA-Z_][a-zA-Z0-9_]*\ze\s*("

" ── Operators ───────────────────────────────────────────────────────────────
syn match   cobrustOperator     "[+\-*/%&|^~]"
syn match   cobrustOperator     "==\|!=\|<=\|>=\|<\|>"
syn match   cobrustOperator     "=\|:="
syn match   cobrustOperator     "->"
syn match   cobrustOperator     "\*\*\|//"

" ── Highlight links ─────────────────────────────────────────────────────────
hi def link cobrustComment      Comment
hi def link cobrustTodo         Todo
hi def link cobrustKeyword      Keyword
hi def link cobrustDefKeyword   Define
hi def link cobrustOpKeyword    Operator
hi def link cobrustBoolean      Boolean
hi def link cobrustNone         Constant
hi def link cobrustType         Type
hi def link cobrustBuiltinType  Type
hi def link cobrustString       String
hi def link cobrustFString      String
hi def link cobrustFStringExpr  Special
hi def link cobrustEscape       SpecialChar
hi def link cobrustNumber       Number
hi def link cobrustDecorator    PreProc
hi def link cobrustFuncDef      Function
hi def link cobrustFuncCall     Function
hi def link cobrustOperator     Operator

let b:current_syntax = "cobrust"
