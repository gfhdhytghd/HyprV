// Copyright (C) 2023 The Qt Company Ltd.
// SPDX-License-Identifier: LGPL-3.0-only
// Installed Qt6.12 MultiEffect GLSL100 source.
#version 100

struct buf
{
    mat4 qt_Matrix;
    float qt_Opacity;
    float shadowScale;
    vec2 shadowOffset;
    vec2 centerOffset;
    float contrast;
    float brightness;
    float saturation;
    vec4 colorizationColor;
    vec4 blurWeight1;
    vec2 blurWeight2;
    vec4 mask;
    float maskInverted;
    vec4 shadowColor;
    vec4 shadowBlurWeight1;
    vec2 shadowBlurWeight2;
};

uniform buf _19;

varying vec2 texCoord;
attribute vec2 qt_MultiTexCoord0;
varying vec2 shadowTexCoord;
attribute vec4 qt_Vertex;

void main()
{
    texCoord = qt_MultiTexCoord0;
    shadowTexCoord = qt_MultiTexCoord0 - _19.shadowOffset;
    shadowTexCoord = (shadowTexCoord * _19.shadowScale) + _19.centerOffset;
    gl_Position = _19.qt_Matrix * qt_Vertex;
}

