/*
 * Stub for FT_Get_BDF_Property and FT_Get_BDF_Charset_ID
 *
 * Fontconfig calls these FreeType BDF functions, but Skia's embedded FreeType
 * doesn't include BDF support. Rather than linking a full FreeType with BDF
 * (which causes duplicate symbol conflicts), we provide stubs that return
 * "not found" - BDF is a legacy bitmap font format we don't need.
 */

/* Minimal type definitions to avoid requiring FreeType headers */
typedef int FT_Error;
typedef void* FT_Face;

typedef struct BDF_PropertyRec_ {
    int type;
    union {
        const char* atom;
        int integer;
        unsigned int cardinal;
    } u;
} BDF_PropertyRec;

#define FT_Err_Invalid_Argument 6

FT_Error FT_Get_BDF_Property(FT_Face face, const char* prop_name, BDF_PropertyRec* aproperty) {
    (void)face;
    (void)prop_name;
    (void)aproperty;
    return FT_Err_Invalid_Argument;
}

FT_Error FT_Get_BDF_Charset_ID(FT_Face face, const char** acharset_encoding, const char** acharset_registry) {
    (void)face;
    (void)acharset_encoding;
    (void)acharset_registry;
    return FT_Err_Invalid_Argument;
}
