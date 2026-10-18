#version 450
layout(location=0) in vec2 uv;
layout(location=1) in vec4 colour;
layout(location=2) flat in vec2 kind_seed;
layout(location=3) in vec3 world;
layout(location=4) flat in vec4 volume;
layout(location=5) flat in float volume_height;
layout(location=0) out vec4 output_colour;
layout(set=1,binding=0,std140) uniform Frame {
    mat4 vp; mat4 reflected_vp; vec4 eye_time; vec4 right_quality; vec4 up_rt;
    vec4 river; vec4 moon; vec4 settings;
};
struct Light { vec4 position; vec4 colour; };
layout(set=1,binding=1,std430) readonly buffer Lights { Light lights[]; };
layout(set=1,binding=12,std430) readonly buffer Depth { uvec2 background[]; };
layout(set=1,binding=2) uniform sampler2D web_alpha;
float noise(vec2 p) {
    vec2 i=floor(p),f=fract(p);f=f*f*(3.-2.*f);
    vec4 n=fract(sin(vec4(dot(i,vec2(127.1,311.7)),dot(i+vec2(1,0),vec2(127.1,311.7)),dot(i+vec2(0,1),vec2(127.1,311.7)),dot(i+1.,vec2(127.1,311.7))))*43758.5453);
    return mix(mix(n.x,n.y,f.x),mix(n.z,n.w,f.x),f.y);
}
void main(){
    float seed=kind_seed.y,kind=kind_seed.x, a; vec3 c=colour.rgb;
    if(kind<.5){
        float y=uv.y*.5+.5,draught=sin(eye_time.w*4.+world.x*.07+world.z*.05)*.11;
        float bend=(draught+sin(eye_time.w*7.7+seed)*.025)*y*y;
        vec2 flow=vec2((uv.x+bend)*5.+seed,y*10.-eye_time.w*3.2);
        float turbulence=noise(flow)*.55+noise(flow*2.03)*.3+noise(flow*4.07)*.15;
        float flicker=.5+.5*sin(eye_time.w*(8.+noise(vec2(seed))*2.)+seed),top=.72+.12*flicker+(turbulence-.5)*.13;
        float width=mix(.58,.055,y)*( .78+.42*turbulence),x=abs(uv.x+bend+(turbulence-.5)*y*.22);
        float body=1.-smoothstep(width*.55,width,x),edge=1.-smoothstep(top-.035,top+.008,y);
        float tip=(1.-smoothstep(width*.3,width*.62,x))*smoothstep(top+.018,top+.055,y)*(1.-smoothstep(top+.1,top+.18,y))*smoothstep(.52,.72,turbulence);
        a=(body*edge+tip*.72)*smoothstep(0.,.055,y)*colour.a;
        float core=pow(max(0.,1.-x/max(width,.01)),3.)*(1.-smoothstep(.46,.82,y));
        c=mix(vec3(.55,.008,.001),vec3(2.2,.25,.008),smoothstep(0.,.72,body));
        c=mix(c,vec3(4.,2.3,.65),core);c*=.84+.2*flicker;
    }else if(kind<1.5){a=exp(-dot(uv,uv)*6.)*(1.-smoothstep(.62,1.,length(uv)))*colour.a;c*=8.;}
    else if(kind<2.5){vec2 q=uv+vec2(noise(uv*2.7+seed+eye_time.w*.13),noise(uv*3.1-seed-eye_time.w*.09))*.22-.11;a=texture(web_alpha,q*.5+.5).g*colour.a*(.45+.55*noise(uv*3.+seed));}
    else if(kind<3.5) { a=pow(max(0.,1.-dot(uv,uv)),3.)*colour.a; }
    else if(kind<4.5) { a=pow(max(0.,1.-dot(uv,uv)),2.)*colour.a; c=vec3(.15,.3,.35)+pow(max(0.,1.-length(uv-vec2(-.2,.25))*3.),8.)*3.; }
    else if(kind<5.5) { a=exp(-pow((length(uv)-.8)*30.,2.))*colour.a; }
    else if(kind<6.5) { a=exp(-pow((length(uv)-.75)*18.,2.))*colour.a;c=vec3(.2,.35,.4); }
    else if(kind<7.5) { a=(1.-smoothstep(.65,.85,abs(uv.x)+abs(uv.y)*.5))*colour.a;c*=.5+.5*cos(uv.y*10.); }
    else if(kind<8.5) {a=texture(web_alpha,uv*.5+.5).r*colour.a;c*=1.2+pow(1.-abs(dot(normalize(eye_time.xyz-world),normalize(cross(dFdx(world),dFdy(world))))),3.);}
    else if(kind<9.5) { vec2 q=vec2(abs(uv.x)-.43,uv.y-.25);float eye=1.-smoothstep(.1,.2,abs(q.x)+abs(q.y)*.6);float mouth=(1.-smoothstep(.09,.15,abs(uv.y+.35+.15*cos(uv.x*5.))))*(1.-smoothstep(.6,.8,abs(uv.x)));a=max(eye,mouth); }
    else if(kind>10.5) {
        vec3 radii=vec3(volume.w,volume_height,volume.w*.65);
        vec3 direction=normalize(world-eye_time.xyz), origin=(eye_time.xyz-volume.xyz)/radii, ray=direction/radii;
        float b=dot(origin,ray), d=b*b-dot(ray,ray)*(dot(origin,origin)-1.);
        if(d<=0.)discard;
        float near=max(0.,(-b-sqrt(d))/dot(ray,ray)), far=(-b+sqrt(d))/dot(ray,ray);
        if(settings.z<.5) {
            ivec2 pixel=ivec2(gl_FragCoord.xy);
            far=min(far,unpackHalf2x16(background[pixel.y*int(settings.x)+pixel.x].y).y);
        }
        float density=0.;int steps=right_quality.w>.5?8:4;
        for(int i=0;i<steps;i++) {
            vec3 p=eye_time.xyz+direction*mix(near,far,(float(i)+.5)/float(steps));
            vec3 q=(p-volume.xyz)/radii;
            float cloud=noise(p.xz*2.+vec2(-eye_time.w*.09,seed)+p.y)*.65+noise(p.xz*4.-eye_time.w*.05)*.35;
            density+=pow(max(0.,1.-dot(q,q)),2.)*smoothstep(.2,.8,cloud);
        }
        a=(1.-exp(-density*max(0.,far-near)*colour.a/float(steps)))*(1.-smoothstep(.65,1.,max(abs(uv.x),abs(uv.y))));
    }
    else { a=float(abs(uv.x)<.9 && uv.y>-.35+abs(uv.x)*.4 && uv.y<.32-abs(uv.x)*.25+.12*abs(sin(uv.x*16.))); }
    if((kind>=2. && kind<9.) || kind>10.5) {
        vec3 illumination=vec3(.1,.13,.16);
        for(int i=0;i<lights.length();i++) {vec3 to=lights[i].position.xyz-world;illumination+=lights[i].colour.rgb*1.4/max(1.,dot(to,to));}
        c*=kind==2.?vec3(.72,.68,.64)+illumination*.08:illumination;
    }
    if(settings.z<.5 && kind<10.5) {
        ivec2 pixel=ivec2(gl_FragCoord.xy);float depth=unpackHalf2x16(background[pixel.y*int(settings.x)+pixel.x].y).y;
        a*=clamp((depth-distance(world,eye_time.xyz))*(kind==2.?2.:30.),0.,1.);
    }
    if((kind==4. || kind==6.) && settings.z<.5) {
        ivec2 p=clamp(ivec2(gl_FragCoord.xy+uv*3.),ivec2(0),ivec2(settings.xy)-1);
        uvec2 refracted_pixel=background[p.y*int(settings.x)+p.x];
        c+=vec3(unpackHalf2x16(refracted_pixel.x),unpackHalf2x16(refracted_pixel.y).x)*.65;
    }
    if(a<.001)discard;
    output_colour=vec4(c*a,kind<1.5?0.:a);
}
