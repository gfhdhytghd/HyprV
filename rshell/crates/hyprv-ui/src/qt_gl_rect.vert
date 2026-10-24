// Copyright (C) 2016 The Qt Company Ltd.
// SPDX-License-Identifier: LGPL-3.0-only
// Installed Qt6.12 smoothcolor QSB, GLSL100 batchable variant.
#version 100

struct buf
{
    mat4 matrix;
    vec2 pixelSize;
    float opacity;
};

uniform buf _14;

attribute vec4 vertex;
attribute vec4 vertexOffset;
varying vec4 color;
attribute vec4 vertexColor;
attribute float _qt_order;

void main()
{
    vec4 pos = _14.matrix * vertex;
    vec4 m0 = _14.matrix[0];
    vec4 m1 = _14.matrix[1];
    gl_Position = pos;
    if (vertexOffset.x != 0.0)
    {
        vec4 delta = m0 * vertexOffset.x;
        vec2 dir = (delta.xy * pos.w) - (pos.xy * delta.w);
        vec2 ndir = (_14.pixelSize * 0.5) * normalize(dir / _14.pixelSize);
        dir -= ((ndir * delta.w) * pos.w);
        float numerator = dot(dir, (ndir * pos.w) * pos.w);
        float scale = 0.0;
        if (numerator < 0.0)
        {
            scale = 1.0;
        }
        else
        {
            scale = min(1.0, numerator / dot(dir, dir));
        }
        gl_Position += (delta * scale);
    }
    if (vertexOffset.y != 0.0)
    {
        vec4 delta_1 = m1 * vertexOffset.y;
        vec2 dir_1 = (delta_1.xy * pos.w) - (pos.xy * delta_1.w);
        vec2 ndir_1 = (_14.pixelSize * 0.5) * normalize(dir_1 / _14.pixelSize);
        dir_1 -= ((ndir_1 * delta_1.w) * pos.w);
        float numerator_1 = dot(dir_1, (ndir_1 * pos.w) * pos.w);
        float scale_1 = 0.0;
        if (numerator_1 < 0.0)
        {
            scale_1 = 1.0;
        }
        else
        {
            scale_1 = min(1.0, numerator_1 / dot(dir_1, dir_1));
        }
        gl_Position += (delta_1 * scale_1);
    }
    color = vertexColor * _14.opacity;
    gl_Position.z = _qt_order * gl_Position.w;
}

