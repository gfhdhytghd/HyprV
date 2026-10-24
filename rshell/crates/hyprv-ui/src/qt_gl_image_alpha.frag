// Copyright (C) 2016 The Qt Company Ltd.
// SPDX-License-Identifier: LGPL-3.0-only
// Installed Qt6.12 texture material, GLSL100.
#version 100
precision highp float;
precision highp int;

struct buf
{
    mat4 qt_Matrix;
    float opacity;
};

uniform buf _23;

uniform highp sampler2D qt_Texture;

varying vec2 qt_TexCoord;

void main()
{
    gl_FragData[0] = texture2D(qt_Texture, qt_TexCoord) * _23.opacity;
}

