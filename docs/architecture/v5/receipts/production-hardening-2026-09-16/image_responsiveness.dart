import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:image/image.dart' as image;
import 'package:flark_fleury/src/image_decode.dart';

Future<void> main(List<String> args) async {
  final fixture=image.Image(width:2048,height:2048);
  for(var y=0;y<2048;y++) {
    for(var x=0;x<2048;x++) {fixture.setPixelRgb(x,y,x%256,y%256,(x+y)%256);}
  }
  final png=Uint8List.fromList(image.encodePng(fixture));
  if (args.isNotEmpty) File(args.single).writeAsBytesSync(png);
  final results=<Object>[];
  for(final mode in ['synchronous','queued','queued','queued']) {
    final clock=Stopwatch()..start();
    var previous=0,maxGap=0,ticks=0;
    final timer=Timer.periodic(const Duration(milliseconds:5),(_) {
      final now=clock.elapsedMicroseconds,gap=now-previous;
      if(gap>maxGap)maxGap=gap;
      previous=now;ticks++;
    });
    late int width;
    if(mode=='synchronous') {
      final decoded=image.decodePng(png)!;
      final resized=image.copyResize(decoded,width:640,height:640,interpolation:image.Interpolation.average);
      image.encodePng(resized);
      width=resized.width;
    } else {
      final prepared=await PreviewDecodeQueue().decode(png,PreviewCancellation());
      width=prepared.image.width;
    }
    final workUs=clock.elapsedMicroseconds;
    final during=ticks;
    await Future<void>.delayed(const Duration(milliseconds:10));
    timer.cancel();
    results.add({'mode':mode,'work_ms':workUs/1000,'ui_ticks_during_work':during,'max_timer_gap_ms':maxGap/1000,'output_width':width});
  }
  print(const JsonEncoder.withIndent('  ').convert({'fixture':'2048x2048 RGB gradient','encoded_bytes':png.length,'timer_period_ms':5,'runs':results,'scope':'Dart native diagnostic, not OS input or frame qualification'}));
}
