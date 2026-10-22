// stars-file-parser — library of shared primitives for parsing Stars! binary files.
//
// All Stars! data files (.r1, .m1–.m16, .x1–.x16, .hst, .xy) share the same
// record-container format and L'Ecuyer LCG cipher.  This module provides those
// shared building blocks so each tool in src/bin/ can focus on field decoding.

pub mod advantage_points;
pub mod cipher;
pub mod gamedef;
pub mod messages;
pub mod name;
pub mod race;
pub mod records;
pub mod universe;
