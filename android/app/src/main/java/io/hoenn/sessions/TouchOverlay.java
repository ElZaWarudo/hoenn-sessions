package io.hoenn.sessions;

import android.content.Context;
import android.content.SharedPreferences;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.RectF;
import android.view.MotionEvent;
import android.view.HapticFeedbackConstants;
import android.view.View;
import java.util.HashMap;
import java.util.Map;

/** Transparent, movable multi-touch controls drawn over the emulator. */
final class TouchOverlay extends View {
    private static final String PREFS = "touch_overlay";
    private static final int FAST_FORWARD = 10;
    private static final int DPAD_POINTER = 0x1000;
    private static final String[] LABELS = {"↑", "↓", "←", "→", "A", "B", "L", "R", "START", "SELECT", "»"};
    private static final int[] MASKS = {64, 128, 32, 16, 1, 2, 512, 256, 8, 4, 0};
    private static final float[][] DEFAULTS = {
        {.14f,.70f},{.14f,.88f},{.07f,.79f},{.21f,.79f},
        {.91f,.80f},{.79f,.80f},{.77f,.62f},{.92f,.62f},
        {.56f,.91f},{.44f,.91f},{.92f,.43f}
    };
    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint text = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final float[][] positions = new float[LABELS.length][2];
    private final Map<Integer,Integer> pointers = new HashMap<>();
    private boolean visible,editing,fastForwardEnabled;
    private volatile boolean controllerPresent;
    private float scale;
    private float opacity;
    private volatile int keys;
    private volatile boolean fastForwardHeld;

    TouchOverlay(Context context) {
        super(context);setFocusable(false);
        text.setColor(0xffe5f0ff);text.setTextAlign(Paint.Align.CENTER);text.setFakeBoldText(true);
        load();
    }

    int keys(){return visible&&!controllerPresent&&!editing?keys:0;}
    boolean fastForwardHeld(){return visible&&!controllerPresent&&!editing&&fastForwardHeld;}
    boolean controlsVisible(){return visible;}
    boolean fastForwardEnabled(){return fastForwardEnabled;}
    float controlScale(){return scale;}
    float controlOpacity(){return opacity;}
    boolean isEditing(){return editing;}

    float[] controlCenter(String label){
        for(int i=0;i<LABELS.length;i++)if(LABELS[i].equals(label)){
            if(i<4){float distance=radius()*.9f;return new float[]{dpadX()+(i==2?-distance:i==3?distance:0),dpadY()+(i==0?-distance:i==1?distance:0)};}
            return new float[]{positions[i][0]*getWidth(),positions[i][1]*getHeight()};
        }
        throw new IllegalArgumentException("Unknown control");
    }
    void setControlsVisible(boolean value){visible=value;if(!visible)clearTouches();save();invalidate();}
    void setControllerPresent(boolean value){controllerPresent=value;if(value)clearTouches();invalidate();}
    void setFastForwardEnabled(boolean value){fastForwardEnabled=value;if(!value)clearTouches();save();invalidate();}
    void setControlScale(float value){scale=Math.max(.65f,Math.min(1.45f,value));save();invalidate();}
    void setControlOpacity(float value){opacity=Math.max(.2f,Math.min(1f,value));save();invalidate();}
    void setEditing(boolean value){editing=value;clearTouches();invalidate();}
    void resetDefaults(){
        visible=true;fastForwardEnabled=false;scale=1f;opacity=1f;
        for(int i=0;i<positions.length;i++){positions[i][0]=DEFAULTS[i][0];positions[i][1]=DEFAULTS[i][1];}
        save();invalidate();
    }

    private float radius(){return Math.min(getWidth(),getHeight())*.073f*scale;}
    private boolean displayed(int index){return index!=FAST_FORWARD||fastForwardEnabled;}
    private RectF shape(int index,float x,float y,float radius){
        if(index==6||index==7)return new RectF(x-radius*1.35f,y-radius*.58f,x+radius*1.35f,y+radius*.58f);
        if(index==8||index==9)return new RectF(x-radius*.72f,y-radius*.27f,x+radius*.72f,y+radius*.27f);
        if(index<4||index==FAST_FORWARD)return new RectF(x-radius*.72f,y-radius*.72f,x+radius*.72f,y+radius*.72f);
        return new RectF(x-radius,y-radius,x+radius,y+radius);
    }
    private float dpadX(){return (positions[0][0]+positions[1][0]+positions[2][0]+positions[3][0])*getWidth()/4f;}
    private float dpadY(){return (positions[0][1]+positions[1][1]+positions[2][1]+positions[3][1])*getHeight()/4f;}
    private int dpadHit(float x,float y){
        float radius=radius(),dx=x-dpadX(),dy=y-dpadY();
        if(Math.abs(dx)>radius*1.55f || Math.abs(dy)>radius*1.55f)return -1;
        if(editing)return 0;
        int directions=0;
        if(dx>radius*.3f)directions|=16;else if(dx<-radius*.3f)directions|=32;
        if(dy>radius*.3f)directions|=128;else if(dy<-radius*.3f)directions|=64;
        return DPAD_POINTER|directions;
    }
    private int hit(float x,float y){
        int dpad=dpadHit(x,y);if(dpad>=0)return dpad;
        float radius=radius(),best=Float.MAX_VALUE;int found=-1;
        for(int i=4;i<positions.length;i++){
            if(!displayed(i))continue;
            float cx=positions[i][0]*getWidth(),cy=positions[i][1]*getHeight();RectF target=shape(i,cx,cy,radius);target.inset(-radius*.28f,-radius*.28f);
            if(target.contains(x,y)){float distance=(x-cx)*(x-cx)+(y-cy)*(y-cy);if(distance<best){best=distance;found=i;}}
        }
        return found;
    }
    private void rebuildKeys(){
        int next=0;boolean fast=false;
        for(int index:pointers.values())if(index>=0){
            if((index&DPAD_POINTER)!=0)next|=index&0xff;
            else if(index==FAST_FORWARD)fast=true;
            else next|=MASKS[index];
        }
        keys=next;fastForwardHeld=fast;
    }
    private void clearTouches(){pointers.clear();keys=0;fastForwardHeld=false;}
    private int faded(int color){return (Math.round(((color>>>24)&255)*opacity)<<24)|(color&0x00ffffff);}

    @Override protected void onDraw(Canvas canvas){
        super.onDraw(canvas);if((!visible||controllerPresent)&&!editing)return;float radius=radius();
        float cx=dpadX(),cy=dpadY();
        RectF horizontal=new RectF(cx-radius*1.45f,cy-radius*.48f,cx+radius*1.45f,cy+radius*.48f);
        RectF vertical=new RectF(cx-radius*.48f,cy-radius*1.45f,cx+radius*.48f,cy+radius*1.45f);
        paint.setStyle(Paint.Style.FILL);paint.setColor(editing?0x663b82c4:faded(0x44000000));
        canvas.drawRoundRect(horizontal,radius*.2f,radius*.2f,paint);canvas.drawRoundRect(vertical,radius*.2f,radius*.2f,paint);
        paint.setStyle(Paint.Style.STROKE);paint.setStrokeWidth(4);paint.setColor(editing?0xff72d3ff:faded(0xdde5f0ff));
        canvas.drawRoundRect(horizontal,radius*.2f,radius*.2f,paint);canvas.drawRoundRect(vertical,radius*.2f,radius*.2f,paint);
        text.setColor(editing?0xffe5f0ff:faded(0xffe5f0ff));text.setTextSize(radius*.5f);
        canvas.drawText("↑",cx,cy-radius*.78f,text);canvas.drawText("↓",cx,cy+radius*1.1f,text);
        canvas.drawText("←",cx-radius*.93f,cy+radius*.18f,text);canvas.drawText("→",cx+radius*.93f,cy+radius*.18f,text);
        for(int i=4;i<LABELS.length;i++){
            if(!displayed(i))continue;
            float x=positions[i][0]*getWidth(),y=positions[i][1]*getHeight();RectF bounds=shape(i,x,y,radius);
            paint.setStyle(Paint.Style.FILL);paint.setColor(editing?0x663b82c4:faded(0x16000000));drawShape(canvas,i,bounds,x,y,radius,paint);
            paint.setStyle(Paint.Style.STROKE);paint.setStrokeWidth(editing?8:7);paint.setColor(editing?0xaa071729:faded(0xaa071729));drawShape(canvas,i,bounds,x,y,radius,paint);
            paint.setStrokeWidth(editing?4:3);paint.setColor(editing?0xff72d3ff:faded(0xdde5f0ff));drawShape(canvas,i,bounds,x,y,radius,paint);
            text.setColor(editing?0xffe5f0ff:faded(0xffe5f0ff));
            text.setTextSize((i==8||i==9)?radius*.31f:radius*.64f);Paint.FontMetrics metrics=text.getFontMetrics();
            canvas.drawText(LABELS[i],x,y-(metrics.ascent+metrics.descent)/2,text);
        }
        if(editing){
            paint.setStyle(Paint.Style.FILL);paint.setColor(0xaa00101f);canvas.drawRect(0,0,getWidth(),radius*.72f,paint);
            text.setTextSize(radius*.3f);canvas.drawText(getContext().getString(R.string.controls_canvas_hint),getWidth()/2f,radius*.48f,text);
        }
    }
    private void drawShape(Canvas canvas,int index,RectF bounds,float x,float y,float radius,Paint target){
        if(index==4||index==5)canvas.drawCircle(x,y,radius,target);
        else{float round=index==6||index==7?radius*.55f:radius*.18f;canvas.drawRoundRect(bounds,round,round,target);}
    }

    @Override public boolean onTouchEvent(MotionEvent event){
        if((!visible||controllerPresent)&&!editing)return false;
        int action=event.getActionMasked(),pointer=event.getActionIndex(),id=event.getPointerId(pointer);
        if(action==MotionEvent.ACTION_DOWN||action==MotionEvent.ACTION_POINTER_DOWN){
            int index=hit(event.getX(pointer),event.getY(pointer));pointers.put(id,index);if(editing&&index>=0)move(index,event.getX(pointer),event.getY(pointer));
            else if(index>=0)performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY);
        }else if(action==MotionEvent.ACTION_MOVE){
            for(int i=0;i<event.getPointerCount();i++){
                int pointerId=event.getPointerId(i);
                Integer index=pointers.get(pointerId);
                if(index==null)continue;
                if(editing){if(index>=0)move(index,event.getX(i),event.getY(i));}
                else pointers.put(pointerId,(index&DPAD_POINTER)!=0?dpadHit(event.getX(i),event.getY(i)):hit(event.getX(i),event.getY(i)));
            }
        }else if(action==MotionEvent.ACTION_UP||action==MotionEvent.ACTION_POINTER_UP){pointers.remove(id);if(editing)save();}
        else if(action==MotionEvent.ACTION_CANCEL)clearTouches();
        rebuildKeys();invalidate();return true;
    }
    private void move(int index,float x,float y){
        if(index<4){
            float mx=radius()*1.6f/Math.max(1,getWidth()),my=radius()*1.6f/Math.max(1,getHeight());
            float cx=Math.max(mx,Math.min(1-mx,x/getWidth())),cy=Math.max(my,Math.min(1-my,y/getHeight()));
            positions[0][0]=cx;positions[0][1]=cy-.09f;
            positions[1][0]=cx;positions[1][1]=cy+.09f;
            positions[2][0]=cx-.07f;positions[2][1]=cy;
            positions[3][0]=cx+.07f;positions[3][1]=cy;
            return;
        }
        float margin=radius()/Math.max(1,getWidth());positions[index][0]=Math.max(margin,Math.min(1-margin,x/getWidth()));
        margin=radius()/Math.max(1,getHeight());positions[index][1]=Math.max(margin,Math.min(1-margin,y/getHeight()));
    }
    private void load(){
        SharedPreferences preferences=getContext().getSharedPreferences(PREFS,Context.MODE_PRIVATE);
        visible=preferences.getBoolean("visible",true);scale=preferences.getFloat("scale",1f);opacity=preferences.getFloat("opacity",1f);fastForwardEnabled=preferences.getBoolean("fast_forward",false);
        for(int i=0;i<positions.length;i++){positions[i][0]=preferences.getFloat("x"+i,DEFAULTS[i][0]);positions[i][1]=preferences.getFloat("y"+i,DEFAULTS[i][1]);}
    }
    private void save(){
        SharedPreferences.Editor editor=getContext().getSharedPreferences(PREFS,Context.MODE_PRIVATE).edit().putBoolean("visible",visible).putBoolean("fast_forward",fastForwardEnabled).putFloat("scale",scale).putFloat("opacity",opacity);
        for(int i=0;i<positions.length;i++)editor.putFloat("x"+i,positions[i][0]).putFloat("y"+i,positions[i][1]);editor.apply();
    }
}
