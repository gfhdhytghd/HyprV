// Copyright (C) 2016 The Qt Company Ltd.
// SPDX-License-Identifier: LGPL-3.0-only
// Installed Qt6.12 smoothcolor QSB, GLSL100 standard variant.
#version 100
precision highp float;
precision highp int;

varying vec4 color;

void main()
{
    gl_FragData[0] = color;
}

