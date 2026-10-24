// Offline private Qt-host GL observation only. Never linked into Rust.
#include <EGL/egl.h>
#include <GLES3/gl32.h>
#include <dlfcn.h>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cmath>
#include <string>
#include <set>
static thread_local bool busy=false;
using Proc=__eglMustCastToProperFunctionPointerType;
static Proc raw(const char*n){static auto f=(Proc(*)(const char*))dlsym(dlopen("libEGL.so.1",RTLD_LAZY|RTLD_LOCAL),"eglGetProcAddress");return f(n);}
static void observe(GLenum mode,GLsizei count,const void*index=nullptr){
 if(busy || !getenv("HYPRV_GL_TRACE_FILE"))return;busy=true;
 auto get=(void(*)(GLenum,GLint*))raw("glGetIntegerv");auto uni=(void(*)(GLuint,GLuint,GLsizei,GLsizei*,GLint*,GLenum*,GLchar*))raw("glGetActiveUniform");auto loc=(GLint(*)(GLuint,const GLchar*))raw("glGetUniformLocation");auto val=(void(*)(GLuint,GLint,GLfloat*))raw("glGetUniformfv");
 GLint p=0,fb=0,vp[4]={};get(GL_CURRENT_PROGRAM,&p);get(GL_DRAW_FRAMEBUFFER_BINDING,&fb);get(GL_VIEWPORT,vp);
 // Restrict to observed popup source FBO. All calls still pass through unchanged.
 if(vp[2]>0 && vp[2]<=1280 && vp[3]<=1024 && (!getenv("HYPRV_GL_TRACE_AT") || (getenv("HYPRV_GL_TRACE_TICK") && fabs(atof(getenv("HYPRV_GL_TRACE_TICK"))-atof(getenv("HYPRV_GL_TRACE_AT")))<0.01))){
  FILE*f=fopen(getenv("HYPRV_GL_TRACE_FILE"),"a");if(f){fprintf(f,"TICK %s\n",getenv("HYPRV_GL_TRACE_TICK")?getenv("HYPRV_GL_TRACE_TICK"):"rust");fprintf(f,"DRAW program=%d fbo=%d vp=%d,%d,%d,%d mode=%u count=%d indexOffset=%zu\n",p,fb,vp[0],vp[1],vp[2],vp[3],mode,count,reinterpret_cast<size_t>(index));GLint n=0;auto prog=(void(*)(GLuint,GLenum,GLint*))raw("glGetProgramiv");prog(p,GL_ACTIVE_UNIFORMS,&n);
   for(GLint i=0;i<n;i++){char name[256];GLsizei len=0;GLint sz=0;GLenum type=0;uni(p,i,256,&len,&sz,&type,name);if(type==GL_FLOAT_MAT4||type==GL_FLOAT_VEC2||type==GL_FLOAT_VEC4||type==GL_FLOAT){GLfloat v[16]={};val(p,loc(p,name),v);fprintf(f," %s type=%x",name,type);for(int k=0;k<(type==GL_FLOAT_MAT4?16:type==GL_FLOAT_VEC4?4:type==GL_FLOAT_VEC2?2:1);k++)fprintf(f," %.9g",v[k]);fprintf(f,"\n");}}
   auto enabled=(GLboolean(*)(GLenum))raw("glIsEnabled");for(auto name:{GL_DITHER,GL_BLEND,GL_DEPTH_TEST,GL_SCISSOR_TEST,GL_SAMPLE_ALPHA_TO_COVERAGE,GL_SAMPLE_COVERAGE})fprintf(f," STATE enabled-%x=%d\n",name,enabled(name));
   for(auto name:{GL_SAMPLES,GL_BLEND_SRC_RGB,GL_BLEND_DST_RGB,GL_BLEND_SRC_ALPHA,GL_BLEND_DST_ALPHA,GL_BLEND_EQUATION_RGB,GL_BLEND_EQUATION_ALPHA}){GLint v=0;get(name,&v);fprintf(f," STATE value-%x=%d\n",name,v);}
   if(fb==1 && (p==3 || p==12)) {
    auto active=(void(*)(GLenum))raw("glActiveTexture");GLint oldActive=0,tex=0;get(GL_ACTIVE_TEXTURE,&oldActive);GLint unit=0;auto getU=(void(*)(GLuint,GLint,GLint*))raw("glGetUniformiv");auto samplerLoc=loc(p,"qt_Texture");if(samplerLoc>=0)getU(p,samplerLoc,&unit);active(GL_TEXTURE0+unit);get(GL_TEXTURE_BINDING_2D,&tex);
    auto filterParam=(void(*)(GLenum,GLenum,GLint*))raw("glGetTexParameteriv");GLint currentMin=0,currentMag=0;filterParam(GL_TEXTURE_2D,GL_TEXTURE_MIN_FILTER,&currentMin);filterParam(GL_TEXTURE_2D,GL_TEXTURE_MAG_FILTER,&currentMag);fprintf(f," CURRENT_TEXTURE program=%d unit=%d tex=%d min=%d mag=%d\n",p,unit,tex,currentMin,currentMag);
    static std::set<std::pair<int,int>> dumped;
    if(tex && dumped.insert({p,tex}).second){
      auto level=(void(*)(GLenum,GLint,GLenum,GLint*))raw("glGetTexLevelParameteriv");GLint w=0,h=0;level(GL_TEXTURE_2D,0,GL_TEXTURE_WIDTH,&w);level(GL_TEXTURE_2D,0,GL_TEXTURE_HEIGHT,&h);
      GLint sampler=0;get(GL_SAMPLER_BINDING,&sampler);auto st=(void(*)(GLenum,GLenum,GLint*))raw("glGetTexParameteriv");auto ss=(void(*)(GLuint,GLenum,GLint*))raw("glGetSamplerParameteriv");
      fprintf(f," SAMPLER texture=%d sampler=%d",tex,sampler);for(auto key:{GL_TEXTURE_MIN_FILTER,GL_TEXTURE_MAG_FILTER,GL_TEXTURE_WRAP_S,GL_TEXTURE_WRAP_T}){GLint value=0;if(sampler)ss(sampler,key,&value);else st(GL_TEXTURE_2D,key,&value);fprintf(f," %x=%d",key,value);}GLint format=0,level1=0;level(GL_TEXTURE_2D,0,GL_TEXTURE_INTERNAL_FORMAT,&format);level(GL_TEXTURE_2D,1,GL_TEXTURE_WIDTH,&level1);fprintf(f," format=%x level1width=%d\n",format,level1);
      auto gen=(void(*)(GLsizei,GLuint*))raw("glGenFramebuffers");auto bindF=(void(*)(GLenum,GLuint))raw("glBindFramebuffer");auto attach=(void(*)(GLenum,GLenum,GLenum,GLuint,GLint))raw("glFramebufferTexture2D");auto read=(void(*)(GLint,GLint,GLsizei,GLsizei,GLenum,GLenum,void*))raw("glReadPixels");auto del=(void(*)(GLsizei,const GLuint*))raw("glDeleteFramebuffers");auto bindB=(void(*)(GLenum,GLuint))raw("glBindBuffer");
      GLint oldRead=0,oldPack=0;get(GL_READ_FRAMEBUFFER_BINDING,&oldRead);get(GL_PIXEL_PACK_BUFFER_BINDING,&oldPack);bindB(GL_PIXEL_PACK_BUFFER,0);GLuint temp=0;gen(1,&temp);bindF(GL_READ_FRAMEBUFFER,temp);attach(GL_READ_FRAMEBUFFER,GL_COLOR_ATTACHMENT0,GL_TEXTURE_2D,tex,0);
      if(w>0 && h>0 && w*h<4*1024*1024){auto pixels=new unsigned char[w*h*4];read(0,0,w,h,GL_RGBA,GL_UNSIGNED_BYTE,pixels);auto path=std::string(getenv("HYPRV_GL_TRACE_FILE"))+".texture-"+std::to_string(tex)+"-"+std::to_string(w)+"x"+std::to_string(h)+".rgba";FILE*o=fopen(path.c_str(),"wb");if(o){fwrite(pixels,1,w*h*4,o);fclose(o);}delete[]pixels;fprintf(f," TEXTURE program=%d id=%d size=%dx%d\n",p,tex,w,h);}
      bindF(GL_READ_FRAMEBUFFER,oldRead);del(1,&temp);bindB(GL_PIXEL_PACK_BUFFER,oldPack);
    }active(oldActive);
   }
   auto attr=(void(*)(GLuint,GLenum,GLint*))raw("glGetVertexAttribiv");auto ptr=(void(*)(GLuint,GLenum,void**))raw("glGetVertexAttribPointerv");
   auto bind=(void(*)(GLenum,GLuint))raw("glBindBuffer");auto param=(void(*)(GLenum,GLenum,GLint*))raw("glGetBufferParameteriv");auto map=(void*(*)(GLenum,GLintptr,GLsizeiptr,GLbitfield))raw("glMapBufferRange");auto unmap=(GLboolean(*)(GLenum))raw("glUnmapBuffer");
   GLint oldCopy=0;get(GL_COPY_READ_BUFFER_BINDING,&oldCopy);
   auto dump=[&](GLint b){if(!b)return;bind(GL_COPY_READ_BUFFER,b);GLint bytes=0;param(GL_COPY_READ_BUFFER,GL_BUFFER_SIZE,&bytes);if(bytes>0 && bytes<4*1024*1024){void*mapped=map(GL_COPY_READ_BUFFER,0,bytes,GL_MAP_READ_BIT);if(mapped){auto path=std::string(getenv("HYPRV_GL_TRACE_FILE"))+".fbo-"+std::to_string(fb)+".program-"+std::to_string(p)+".buffer-"+std::to_string(b);FILE*o=fopen(path.c_str(),"wb");if(o){fwrite(mapped,1,bytes,o);fclose(o);}unmap(GL_COPY_READ_BUFFER);}}};
   for(GLuint a=0;a<8;a++){GLint en=0,b=0,stride=0,type=0,size=0;void*offset=nullptr;attr(a,GL_VERTEX_ATTRIB_ARRAY_ENABLED,&en);if(!en)continue;attr(a,GL_VERTEX_ATTRIB_ARRAY_BUFFER_BINDING,&b);attr(a,GL_VERTEX_ATTRIB_ARRAY_STRIDE,&stride);attr(a,GL_VERTEX_ATTRIB_ARRAY_TYPE,&type);attr(a,GL_VERTEX_ATTRIB_ARRAY_SIZE,&size);ptr(a,GL_VERTEX_ATTRIB_ARRAY_POINTER,&offset);fprintf(f," ATTR %u buffer=%d stride=%d type=%x size=%d offset=%zu\n",a,b,stride,type,size,reinterpret_cast<size_t>(offset));dump(b);}
   GLint idx=0;get(GL_ELEMENT_ARRAY_BUFFER_BINDING,&idx);fprintf(f," INDEX buffer=%d\n",idx);dump(idx);bind(GL_COPY_READ_BUFFER,oldCopy);
   fclose(f);}
 }
 busy=false;
}
extern "C" void glDrawElements(GLenum m,GLsizei c,GLenum t,const void*i){observe(m,c,i);((void(*)(GLenum,GLsizei,GLenum,const void*))raw("glDrawElements"))(m,c,t,i);}
extern "C" void glDrawArrays(GLenum m,GLint first,GLsizei c){observe(m,c);((void(*)(GLenum,GLint,GLsizei))raw("glDrawArrays"))(m,first,c);}
extern "C" void glShaderSource(GLuint shader,GLsizei count,const GLchar*const*strings,const GLint*lengths){
 if(auto path=getenv("HYPRV_GL_TRACE_FILE")){std::string s;for(int i=0;i<count;i++)s.append(strings[i],lengths&&lengths[i]>=0?lengths[i]:strlen(strings[i]));FILE*f=fopen((std::string(path)+".shader-"+std::to_string(shader)).c_str(),"wb");if(f){fwrite(s.data(),1,s.size(),f);fclose(f);}}
 ((void(*)(GLuint,GLsizei,const GLchar*const*,const GLint*))raw("glShaderSource"))(shader,count,strings,lengths);
}
extern "C" void glAttachShader(GLuint program,GLuint shader){
 if(auto path=getenv("HYPRV_GL_TRACE_FILE")){FILE*f=fopen(path,"a");if(f){fprintf(f,"ATTACH program=%u shader=%u\n",program,shader);fclose(f);}}
 ((void(*)(GLuint,GLuint))raw("glAttachShader"))(program,shader);
}
extern "C" Proc eglGetProcAddress(const char*n){if(!strcmp(n,"glDrawElements"))return (Proc)glDrawElements;if(!strcmp(n,"glDrawArrays"))return (Proc)glDrawArrays;if(!strcmp(n,"glShaderSource"))return(Proc)glShaderSource;if(!strcmp(n,"glAttachShader"))return(Proc)glAttachShader;return raw(n);}
