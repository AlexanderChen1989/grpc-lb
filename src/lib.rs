use std::{
    ffi::{CStr, CString},
    mem,
};


use libc::{c_char, c_int, size_t};

pub mod pb;

#[no_mangle]
pub extern "C" fn hello() {
    println!("Hello, world!");
}

#[no_mangle]
pub unsafe extern "C" fn say_hi(name: *const c_char) {
    let name = CStr::from_ptr(name).to_str().unwrap();
    println!("Hello, {}!", name);
}

#[no_mangle]
pub unsafe extern "C" fn say_nums(
    name: *const c_int,
    size: c_int,
) {
    for i in 0..size as isize {
        let num = *name.offset(i);
        println!("{}", num);
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct Nums {
    pub num: *const c_int,
    pub cap: size_t,
    pub size: size_t,
}

#[no_mangle]
pub unsafe extern "C" fn name() -> *const c_char {
    let s = CString::new("Alexander").unwrap();
    s.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn okok() -> Nums {
    let v = vec![1, 2, 3, 4, 5, 6];

    let nums = Nums {
        num: v.as_ptr(),
        cap: v.capacity(),
        size: v.len(),
    };

    mem::forget(v);

    return nums;
}

#[no_mangle]
pub unsafe extern "C" fn hi_nums(nums: Nums) {
    let v: Vec<i32> = Vec::from_raw_parts(nums.num as *mut i32, nums.size, nums.cap);
    println!("{:?}", v);
    println!("{:?}", v);
    println!("{:?}", v);
    println!("{:?}", v);
}
