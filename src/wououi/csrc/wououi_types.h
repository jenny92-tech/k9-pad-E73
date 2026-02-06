/**
 * Minimal type definitions for bare-metal compilation
 * Replaces standard library headers
 */

#ifndef __WOUOUI_TYPES_H__
#define __WOUOUI_TYPES_H__

// Integer types
typedef signed char         int8_t;
typedef short               int16_t;
typedef int                 int32_t;
typedef long long           int64_t;

typedef unsigned char       uint8_t;
typedef unsigned short      uint16_t;
typedef unsigned int        uint32_t;
typedef unsigned long long  uint64_t;

// Boolean type - use macro instead of typedef for C23 compatibility
#ifndef __bool_true_false_are_defined
#define bool  _Bool
#define true  1
#define false 0
#define __bool_true_false_are_defined 1
#endif

// NULL
#ifndef NULL
#define NULL ((void*)0)
#endif

// Size type
typedef unsigned int size_t;

#endif // __WOUOUI_TYPES_H__
