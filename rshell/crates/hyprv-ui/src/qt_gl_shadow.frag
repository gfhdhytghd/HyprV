// Copyright (C) 2023 The Qt Company Ltd.
// SPDX-License-Identifier: LGPL-3.0-only
// Installed Qt6.12 multieffect_cs2.frag.qsb, GLSL100 standard variant.
#version 100
precision highp float;
precision highp int;

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

uniform buf _36;

uniform highp sampler2D src;
uniform highp sampler2D blurSrc1;
uniform highp sampler2D blurSrc2;
uniform highp sampler2D blurSrc3;
uniform highp sampler2D blurSrc4;

varying vec2 uv;
varying vec2 shadowUV;

uniform float flip0;
vec4 sample0(vec2 p){return texture2D(src,vec2(p.x,mix(p.y,1.0-p.y,flip0)));}
uniform float flip1;
vec4 sample1(vec2 p){return texture2D(blurSrc1,vec2(p.x,mix(p.y,1.0-p.y,flip1)));}
uniform float flip2;
vec4 sample2(vec2 p){return texture2D(blurSrc2,vec2(p.x,mix(p.y,1.0-p.y,flip2)));}
uniform float flip3;
vec4 sample3(vec2 p){return texture2D(blurSrc3,vec2(p.x,mix(p.y,1.0-p.y,flip3)));}
uniform float flip4;
vec4 sample4(vec2 p){return texture2D(blurSrc4,vec2(p.x,mix(p.y,1.0-p.y,flip4)));}
void main()
{
    vec4 color = sample0( uv);
    vec4 _21 = color;
    float _28 = color.w;
    float _45 = color.w;
    vec3 _48 = ((_21.xyz - vec3(0.5 * _28)) * (1.0 + _36.contrast)) + vec3(0.5 * _45);
    color.x = _48.x;
    color.y = _48.y;
    color.z = _48.z;
    float _62 = color.w;
    vec4 _64 = color;
    vec3 _67 = _64.xyz + vec3(_36.brightness * _62);
    color.x = _67.x;
    color.y = _67.y;
    color.z = _67.z;
    float gray = dot(color.xyz, vec3(0.2989999949932098388671875, 0.58700001239776611328125, 0.114000000059604644775390625));
    vec4 _92 = color;
    vec3 _98 = ((_36.colorizationColor.xyz * gray) * _36.colorizationColor.w) + (_92.xyz * (1.0 - _36.colorizationColor.w));
    color.x = _98.x;
    color.y = _98.y;
    color.z = _98.z;
    vec4 _107 = color;
    vec3 _114 = mix(vec3(gray), _107.xyz, vec3(1.0 + _36.saturation));
    color.x = _114.x;
    color.y = _114.y;
    color.z = _114.z;
    float shadow = sample0( shadowUV).w * _36.shadowBlurWeight1.x;
    shadow += (sample1( shadowUV).w * _36.shadowBlurWeight1.y);
    shadow += (sample2( shadowUV).w * _36.shadowBlurWeight1.z);
    shadow += (sample3( shadowUV).w * _36.shadowBlurWeight1.w);
    shadow += (sample4( shadowUV).w * _36.shadowBlurWeight2.x);
    float aa = color.w + ((1.0 - color.w) * (1.0 - shadow));
    aa = min(1.0, aa);
    color = mix(_36.shadowColor, color, vec4(aa));
    gl_FragData[0] = color * _36.qt_Opacity;
}
