package io.hoenn.sessions;

import android.content.Context;
import android.content.SharedPreferences;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import android.view.MotionEvent;
import android.view.View;
import java.util.HashMap;
import java.util.Map;

/** Multi-touch GBA controls drawn over the emulator surface. */
final class TouchOverlay extends View {
    private static final String PREFS = "touch_overlay";
    private static final String[] LABELS = {"↑", "↓", "←", "→", "A", "B", "L", "R", "START", "SELECT"};
    private static final int[] MASKS = {64, 128, 32, 16, 1, 2, 512, 256, 8, 4};
    private static final float[][] DEFAULTS = {
        {.18f,.64f},{.18f,.86f},{.08f,.75f},{.28f,.75f},{.86f,.68f},{.72f,.80f},
        {.14f,.13f},{.86f,.13f},{.58f,.91f},{.40f,.91f}
    };
    private final Paint fill = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint text = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final float[][] positions = new float[LABELS.length][2];
    private final Map<Integer,Integer> pointers = new HashMap<>();
    private boolean visible;
    private boolean editing;
    private float scale;
    private int keys;

    TouchOverlay(Context context) {
        super(context);
        setFocusable(false);
        text.setColor(Color.WHITE);
        text.setTextAlign(Paint.Align.CENTER);
        load();
    }

    int keys() { return visible && !editing ? keys : 0; }
    boolean controlsVisible() { return visible; }
    float controlScale() { return scale; }
    boolean isEditing() { return editing; }

    float[] controlCenter(String label) {
        for(int i=0;i<LABELS.length;i++)if(LABELS[i].equals(label))return new float[]{positions[i][0]*getWidth(),positions[i][1]*getHeight()};
        throw new IllegalArgumentException("Unknown control");
    }

    void setControlsVisible(boolean value) {
        visible = value;
        if (!visible) clearTouches();
        save();
        invalidate();
    }

    void setControlScale(float value) {
        scale = Math.max(.65f, Math.min(1.45f, value));
        save();
        invalidate();
    }

    void setEditing(boolean value) {
        editing = value;
        clearTouches();
        invalidate();
    }

    void resetPositions() {
        for (int i=0;i<positions.length;i++) {
            positions[i][0]=DEFAULTS[i][0]; positions[i][1]=DEFAULTS[i][1];
        }
        save(); invalidate();
    }

    private float radius() { return Math.min(getWidth(),getHeight()) * .075f * scale; }
    private int hit(float x,float y) {
        float r=radius()*1.25f, best=r*r; int found=-1;
        for(int i=0;i<positions.length;i++){
            float dx=x-positions[i][0]*getWidth(),dy=y-positions[i][1]*getHeight(),distance=dx*dx+dy*dy;
            if(distance<best){best=distance;found=i;}
        }
        return found;
    }

    private void rebuildKeys(){keys=0;for(int index:pointers.values())if(index>=0)keys|=MASKS[index];}
    private void clearTouches(){pointers.clear();keys=0;}

    @Override protected void onDraw(Canvas canvas) {
        super.onDraw(canvas);
        if(!visible && !editing)return;
        float r=radius(); text.setTextSize(r*.62f);
        for(int i=0;i<LABELS.length;i++){
            float x=positions[i][0]*getWidth(),y=positions[i][1]*getHeight();
            fill.setColor(editing?0xcc1976d2:0x77202020);
            canvas.drawCircle(x,y,r,fill);
            fill.setStyle(Paint.Style.STROKE);fill.setStrokeWidth(editing?5:2);fill.setColor(0xccffffff);
            canvas.drawCircle(x,y,r,fill);fill.setStyle(Paint.Style.FILL);
            Paint.FontMetrics f=text.getFontMetrics();canvas.drawText(LABELS[i],x,y-(f.ascent+f.descent)/2,text);
        }
        if(editing){
            fill.setColor(0xaa000000);canvas.drawRect(new RectF(0,0,getWidth(),radius()),fill);
            text.setTextSize(radius()*.34f);canvas.drawText("Arrastra cada control y pulsa Guardar",getWidth()/2f,radius()*.62f,text);
        }
    }

    @Override public boolean onTouchEvent(MotionEvent event) {
        if(!visible && !editing)return false;
        int action=event.getActionMasked(),pointer=event.getActionIndex(),id=event.getPointerId(pointer);
        if(action==MotionEvent.ACTION_DOWN || action==MotionEvent.ACTION_POINTER_DOWN){
            int index=hit(event.getX(pointer),event.getY(pointer));pointers.put(id,index);
            if(editing && index>=0)move(index,event.getX(pointer),event.getY(pointer));
        }else if(action==MotionEvent.ACTION_MOVE){
            for(int i=0;i<event.getPointerCount();i++){
                Integer index=pointers.get(event.getPointerId(i));
                if(editing && index!=null && index>=0)move(index,event.getX(i),event.getY(i));
            }
        }else if(action==MotionEvent.ACTION_UP || action==MotionEvent.ACTION_POINTER_UP){
            pointers.remove(id);if(editing)save();
        }else if(action==MotionEvent.ACTION_CANCEL)clearTouches();
        rebuildKeys();invalidate();return true;
    }

    private void move(int index,float x,float y){
        float margin=radius()/Math.max(1,getWidth());
        positions[index][0]=Math.max(margin,Math.min(1-margin,x/getWidth()));
        margin=radius()/Math.max(1,getHeight());
        positions[index][1]=Math.max(margin,Math.min(1-margin,y/getHeight()));
    }

    private void load(){
        SharedPreferences p=getContext().getSharedPreferences(PREFS,Context.MODE_PRIVATE);
        visible=p.getBoolean("visible",true);scale=p.getFloat("scale",1f);
        for(int i=0;i<positions.length;i++){
            positions[i][0]=p.getFloat("x"+i,DEFAULTS[i][0]);positions[i][1]=p.getFloat("y"+i,DEFAULTS[i][1]);
        }
    }

    private void save(){
        SharedPreferences.Editor e=getContext().getSharedPreferences(PREFS,Context.MODE_PRIVATE).edit()
            .putBoolean("visible",visible).putFloat("scale",scale);
        for(int i=0;i<positions.length;i++)e.putFloat("x"+i,positions[i][0]).putFloat("y"+i,positions[i][1]);
        e.apply();
    }
}
