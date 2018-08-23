use std::env;
use std::fmt::Write;
use std::mem;
use std::os::raw::{c_int, c_ulong, c_void};
use std::slice::{self, SliceConcatExt};

use lldb::*;
use regex::{Captures, Regex, RegexBuilder};

use crate::debug_session::Evaluated;
use crate::error::Error;
use crate::lldb::*;
use crate::must_initialize::*;

pub fn initialize(interpreter: &SBCommandInterpreter) -> Result<(), Error> {
    let mut init_script = env::current_exe()?;
    init_script.set_file_name("codelldb.py");

    let mut command_result = SBCommandReturnObject::new();
    let command = format!("command script import '{}'", init_script.to_str()?);
    interpreter.handle_command(&command, &mut command_result, false);
    info!("{:?}", command_result);
    Ok(())
}

type EvalResult = Result<Evaluated, String>;

pub fn evaluate(
    interpreter: &SBCommandInterpreter, script: &str, simple_expr: bool, context: &SBExecutionContext,
) -> EvalResult {
    extern "C" fn callback(ty: c_int, data: *const c_void, len: usize, result_ptr: *mut EvalResult) {
        unsafe {
            *result_ptr = match ty {
                1 => {
                    let sbvalue = data as *const SBValue;
                    Ok(Evaluated::SBValue((*sbvalue).clone()))
                }
                2 => {
                    let bytes = slice::from_raw_parts(data as *const u8, len);
                    Ok(Evaluated::String(String::from_utf8_lossy(bytes).into_owned()))
                }
                3 => {
                    let bytes = slice::from_raw_parts(data as *const u8, len);
                    Err(String::from_utf8_lossy(bytes).into_owned())
                }
                _ => unreachable!(),
            }
        }
    }

    let mut eval_result = Err(String::new());

    let command = format!(
        "script codelldb.evaluate('{}',{},{:#X},{:#X})",
        script,
        if simple_expr { "True" } else { "False" },
        callback as *mut c_void as usize,
        &mut eval_result as *mut EvalResult as usize
    );

    let mut command_result = SBCommandReturnObject::new();
    let result = interpreter.handle_command_with_context(&command, &context, &mut command_result, false);

    info!("{:?}", command_result);
    info!("{:?}", eval_result);
    eval_result
}

pub fn modules_loaded(interpreter: &SBCommandInterpreter, modules: &mut Iterator<Item = &SBModule>) {
    extern "C" fn assign_sbmodule(dest: *mut SBModule, src: *const SBModule) {
        unsafe {
            *dest = (*src).clone();
        }
    }

    let module_addrs = modules.fold(String::new(), |mut s, m| {
        if !s.is_empty() {
            s.push(',');
        }
        write!(s, "{:#X}", m as *const SBModule as usize);
        s
    });
    info!("{}", module_addrs);

    let mut command_result = SBCommandReturnObject::new();
    let command = format!(
        "script codelldb.modules_loaded([{}],{:#X})",
        module_addrs, assign_sbmodule as *mut c_void as usize,
    );
    let result = interpreter.handle_command(&command, &mut command_result, false);
    debug!("{:?}", command_result);
}

fn create_regexes() -> [Regex; 3] {
    // Matches Python strings
    let pystring = [
        r#"(?:"(?:\\"|\\\\|[^"])*")"#,
        r#"(?:'(?:\\'|\\\\|[^'])*')"#,
        r#"(?:r"[^"]*")"#,
        r#"(?:r'[^']*')"#,
    ]
        .join("|");

    let kwlist = [
        "as", "assert", "break", "class", "continue", "def", "del", "elif", "else", "except", "exec", "finally", "for",
        "from", "global", "if", "import", "in", "is", "lambda", "pass", "print", "raise", "return", "try", "while",
        "with", "yield", // except "and", "or", "not"
    ];

    // # Matches Python keywords
    let keywords = kwlist.join("|");

    // # Matches identifiers
    let ident = r#"[A-Za-z_] [A-Za-z0-9_]*"#;

    // # Matches `::xxx`, `xxx::yyy`, `::xxx::yyy`, `xxx::yyy::zzz`, etc
    let qualified_ident = format!(r#"(?: (?: ::)? (?: {ident} ::)+ | :: ) {ident}"#, ident = ident);

    // # Matches `xxx`, `::xxx`, `xxx::yyy`, `::xxx::yyy`, `xxx::yyy::zzz`, etc
    let maybe_qualified_ident = format!(r#"(?: ::)? (?: {ident} ::)* {ident}"#, ident = ident);

    // # Matches `$xxx`, `$xxx::yyy::zzz` or `${...}`, captures the escaped text.
    let escaped_ident = format!(
        r#"\$ ({maybe_qualified_ident}) | \$ \{{ ([^}}]*) \}}"#,
        maybe_qualified_ident = maybe_qualified_ident
    );

    let maybe_qualified_ident = format!(
        r#"^ {maybe_qualified_ident} $"#,
        maybe_qualified_ident = maybe_qualified_ident
    );

    let preprocess_simple = format!(
        r#"(\.)? (?: {pystring} | \b ({keywords}) \b | ({qualified_ident}) | {escaped_ident} )"#,
        pystring = pystring,
        keywords = keywords,
        qualified_ident = qualified_ident,
        escaped_ident = escaped_ident
    );

    let preprocess_python = format!(
        r#"(\.)? (?: {pystring} | {escaped_ident} )"#,
        pystring = pystring,
        escaped_ident = escaped_ident
    );

    [
        RegexBuilder::new(&maybe_qualified_ident)
            .ignore_whitespace(true)
            .build()
            .unwrap(),
        RegexBuilder::new(&preprocess_simple)
            .ignore_whitespace(true)
            .build()
            .unwrap(),
        RegexBuilder::new(&preprocess_python)
            .ignore_whitespace(true)
            .build()
            .unwrap(),
    ]
}

lazy_static! {
    static ref EXPRESSIONS: [Regex; 3] = create_regexes();
    static ref MAYBE_QUALIFIED_IDENT: &'static Regex = &EXPRESSIONS[0];
    static ref PREPROCESS_SIMPLE: &'static Regex = &EXPRESSIONS[1];
    static ref PREPROCESS_PYTHON: &'static Regex = &EXPRESSIONS[2];
}

fn replacer(captures: &Captures) -> String {
    let mut iter = captures.iter();
    iter.next(); // Skip the full match
    let have_prefix = iter.next().unwrap().is_some();
    for ident in iter {
        if let Some(ident) = ident {
            if have_prefix {
                return format!(r#".__getattr__("{}")"#, ident.as_str());
            } else {
                return format!(r#"__frame_vars["{}"]"#, ident.as_str());
            }
        }
    }
    return captures[0].into();
}

// Replaces identifiers that are invalid according to Python syntax in simple expressions:
// - identifiers that happen to be Python keywords (e.g.`for`),
// - qualified identifiers (e.g. `foo::bar::baz`),
// - raw identifiers of the form $xxxxxx,
// with access via `__frame_vars`, or `__getattr__()` (if prefixed by a dot).
// For example, `for + foo::bar::baz + foo::bar::baz.class() + $SomeClass<int>::value` will be translated to
// `__frame_vars["for"] + __frame_vars["foo::bar::baz"] +
//  __frame_vars["foo::bar::baz"].__getattr__("class") + __frame_vars["SomeClass<int>::value"]`
pub fn preprocess_simple_expr(expr: &str) -> String {
    // TODO: Cow?
    PREPROCESS_SIMPLE.replace(expr, replacer).into_owned()
}

// Replaces variable placeholders in native Python expressions with access via __frame_vars,
// or `__getattr__()` (if prefixed by a dot).
// For example, `$var + 42` will be translated to `__frame_vars["var"] + 42`.
pub fn preprocess_python_expr(expr: &str) -> String {
    PREPROCESS_PYTHON.replace(expr, replacer).into_owned()
}

#[test]
fn test_simple() {
    let expr = r#"
        class = from.global.finally
        local::bar::__BAZ()
        local_string()
        ::foo
        ::foo::bar::baz
        foo::bar::baz
        $local::foo::bar
        ${std::integral_constant<long, 1l>::value}
        ${std::integral_constant<long, 1l, foo<123>>::value}
        ${std::allocator_traits<std::allocator<std::thread::_Impl<std::_Bind_simple<threads(int)::__lambda0(int)> > > >::__construct_helper<std::thread::_Impl<std::_Bind_simple<threads(int)::__lambda0(int)> >, std::_Bind_simple<threads(int)::__lambda0(int)> >::value}
        vec_int.${std::_Vector_base<std::vector<int, std::allocator<int> >, std::allocator<std::vector<int, std::allocator<int> > > >}._M_impl._M_start

        """continue.exec = pass.print; yield.with = 3"""
        \'''continue.exec = pass.print; yield.with = 3\'''
        "continue.exec = pass.print; yield.with = 3"
    "#;
    let expected = r#"
        __frame_vars["class"] = __frame_vars["from"].__getattr__("global").__getattr__("finally")
        __frame_vars["local::bar::__BAZ"]()
        local_string()
        __frame_vars["::foo"]
        __frame_vars["::foo::bar::baz"]
        __frame_vars["foo::bar::baz"]
        __frame_vars["local::foo::bar"]
        __frame_vars["std::integral_constant<long, 1l>::value"]
        __frame_vars["std::integral_constant<long, 1l, foo<123>>::value"]
        __frame_vars["std::allocator_traits<std::allocator<std::thread::_Impl<std::_Bind_simple<threads(int)::__lambda0(int)> > > >::__construct_helper<std::thread::_Impl<std::_Bind_simple<threads(int)::__lambda0(int)> >, std::_Bind_simple<threads(int)::__lambda0(int)> >::value"]
        vec_int.__getattr__("std::_Vector_base<std::vector<int, std::allocator<int> >, std::allocator<std::vector<int, std::allocator<int> > > >")._M_impl._M_start

        """continue.exec = pass.print; yield.with = 3"""
        \'''continue.exec = pass.print; yield.with = 3\'''
        "continue.exec = pass.print; yield.with = 3"
    "#;
    let prepr = preprocess_simple_expr(expr);

    assert_eq!(expected, prepr);
}

#[test]
fn test_python() {
    let expr = r#"
        for x in $foo: print x
        $xxx.$yyy.$zzz
        $xxx::yyy::zzz
        $::xxx
        "$xxx::yyy::zzz"
    "#;
    let expected = r#"
        for x in __frame_vars["foo"]: print x
        __frame_vars["xxx"].__getattr__("yyy").__getattr__("zzz")
        __frame_vars["xxx::yyy::zzz"]
        __frame_vars["::xxx"]
        "$xxx::yyy::zzz"
    "#;
    let prepr = preprocess_python_expr(expr);
    assert_eq!(expected, prepr);
}
