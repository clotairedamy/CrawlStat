// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'project.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$ReportEvent {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is ReportEvent);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'ReportEvent()';
}


}

/// @nodoc
class $ReportEventCopyWith<$Res>  {
$ReportEventCopyWith(ReportEvent _, $Res Function(ReportEvent) __);
}


/// Adds pattern-matching-related methods to [ReportEvent].
extension ReportEventPatterns on ReportEvent {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( ReportEvent_Model value)?  model,TResult Function( ReportEvent_Progress value)?  progress,TResult Function( ReportEvent_Done value)?  done,TResult Function( ReportEvent_Failed value)?  failed,required TResult orElse(),}){
final _that = this;
switch (_that) {
case ReportEvent_Model() when model != null:
return model(_that);case ReportEvent_Progress() when progress != null:
return progress(_that);case ReportEvent_Done() when done != null:
return done(_that);case ReportEvent_Failed() when failed != null:
return failed(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( ReportEvent_Model value)  model,required TResult Function( ReportEvent_Progress value)  progress,required TResult Function( ReportEvent_Done value)  done,required TResult Function( ReportEvent_Failed value)  failed,}){
final _that = this;
switch (_that) {
case ReportEvent_Model():
return model(_that);case ReportEvent_Progress():
return progress(_that);case ReportEvent_Done():
return done(_that);case ReportEvent_Failed():
return failed(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( ReportEvent_Model value)?  model,TResult? Function( ReportEvent_Progress value)?  progress,TResult? Function( ReportEvent_Done value)?  done,TResult? Function( ReportEvent_Failed value)?  failed,}){
final _that = this;
switch (_that) {
case ReportEvent_Model() when model != null:
return model(_that);case ReportEvent_Progress() when progress != null:
return progress(_that);case ReportEvent_Done() when done != null:
return done(_that);case ReportEvent_Failed() when failed != null:
return failed(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String name)?  model,TResult Function( String text)?  progress,TResult Function( String text)?  done,TResult Function( String message)?  failed,required TResult orElse(),}) {final _that = this;
switch (_that) {
case ReportEvent_Model() when model != null:
return model(_that.name);case ReportEvent_Progress() when progress != null:
return progress(_that.text);case ReportEvent_Done() when done != null:
return done(_that.text);case ReportEvent_Failed() when failed != null:
return failed(_that.message);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String name)  model,required TResult Function( String text)  progress,required TResult Function( String text)  done,required TResult Function( String message)  failed,}) {final _that = this;
switch (_that) {
case ReportEvent_Model():
return model(_that.name);case ReportEvent_Progress():
return progress(_that.text);case ReportEvent_Done():
return done(_that.text);case ReportEvent_Failed():
return failed(_that.message);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String name)?  model,TResult? Function( String text)?  progress,TResult? Function( String text)?  done,TResult? Function( String message)?  failed,}) {final _that = this;
switch (_that) {
case ReportEvent_Model() when model != null:
return model(_that.name);case ReportEvent_Progress() when progress != null:
return progress(_that.text);case ReportEvent_Done() when done != null:
return done(_that.text);case ReportEvent_Failed() when failed != null:
return failed(_that.message);case _:
  return null;

}
}

}

/// @nodoc


class ReportEvent_Model extends ReportEvent {
  const ReportEvent_Model({required this.name}): super._();
  

 final  String name;

/// Create a copy of ReportEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ReportEvent_ModelCopyWith<ReportEvent_Model> get copyWith => _$ReportEvent_ModelCopyWithImpl<ReportEvent_Model>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is ReportEvent_Model&&(identical(other.name, name) || other.name == name));
}


@override
int get hashCode {
    return Object.hash(runtimeType,name);
}

@override
String toString() {
    return 'ReportEvent.model(name: $name)';
}


}

/// @nodoc
abstract mixin class $ReportEvent_ModelCopyWith<$Res> implements $ReportEventCopyWith<$Res> {
  factory $ReportEvent_ModelCopyWith(ReportEvent_Model value, $Res Function(ReportEvent_Model) _then) = _$ReportEvent_ModelCopyWithImpl;
@useResult
$Res call({
 String name
});




}
/// @nodoc
class _$ReportEvent_ModelCopyWithImpl<$Res>
    implements $ReportEvent_ModelCopyWith<$Res> {
  _$ReportEvent_ModelCopyWithImpl(this._self, this._then);

  final ReportEvent_Model _self;
  final $Res Function(ReportEvent_Model) _then;

/// Create a copy of ReportEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? name = null,}) {
  return _then(ReportEvent_Model(
name: null == name ? _self.name : name // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class ReportEvent_Progress extends ReportEvent {
  const ReportEvent_Progress({required this.text}): super._();
  

 final  String text;

/// Create a copy of ReportEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ReportEvent_ProgressCopyWith<ReportEvent_Progress> get copyWith => _$ReportEvent_ProgressCopyWithImpl<ReportEvent_Progress>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is ReportEvent_Progress&&(identical(other.text, text) || other.text == text));
}


@override
int get hashCode {
    return Object.hash(runtimeType,text);
}

@override
String toString() {
    return 'ReportEvent.progress(text: $text)';
}


}

/// @nodoc
abstract mixin class $ReportEvent_ProgressCopyWith<$Res> implements $ReportEventCopyWith<$Res> {
  factory $ReportEvent_ProgressCopyWith(ReportEvent_Progress value, $Res Function(ReportEvent_Progress) _then) = _$ReportEvent_ProgressCopyWithImpl;
@useResult
$Res call({
 String text
});




}
/// @nodoc
class _$ReportEvent_ProgressCopyWithImpl<$Res>
    implements $ReportEvent_ProgressCopyWith<$Res> {
  _$ReportEvent_ProgressCopyWithImpl(this._self, this._then);

  final ReportEvent_Progress _self;
  final $Res Function(ReportEvent_Progress) _then;

/// Create a copy of ReportEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? text = null,}) {
  return _then(ReportEvent_Progress(
text: null == text ? _self.text : text // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class ReportEvent_Done extends ReportEvent {
  const ReportEvent_Done({required this.text}): super._();
  

 final  String text;

/// Create a copy of ReportEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ReportEvent_DoneCopyWith<ReportEvent_Done> get copyWith => _$ReportEvent_DoneCopyWithImpl<ReportEvent_Done>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is ReportEvent_Done&&(identical(other.text, text) || other.text == text));
}


@override
int get hashCode {
    return Object.hash(runtimeType,text);
}

@override
String toString() {
    return 'ReportEvent.done(text: $text)';
}


}

/// @nodoc
abstract mixin class $ReportEvent_DoneCopyWith<$Res> implements $ReportEventCopyWith<$Res> {
  factory $ReportEvent_DoneCopyWith(ReportEvent_Done value, $Res Function(ReportEvent_Done) _then) = _$ReportEvent_DoneCopyWithImpl;
@useResult
$Res call({
 String text
});




}
/// @nodoc
class _$ReportEvent_DoneCopyWithImpl<$Res>
    implements $ReportEvent_DoneCopyWith<$Res> {
  _$ReportEvent_DoneCopyWithImpl(this._self, this._then);

  final ReportEvent_Done _self;
  final $Res Function(ReportEvent_Done) _then;

/// Create a copy of ReportEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? text = null,}) {
  return _then(ReportEvent_Done(
text: null == text ? _self.text : text // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class ReportEvent_Failed extends ReportEvent {
  const ReportEvent_Failed({required this.message}): super._();
  

 final  String message;

/// Create a copy of ReportEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ReportEvent_FailedCopyWith<ReportEvent_Failed> get copyWith => _$ReportEvent_FailedCopyWithImpl<ReportEvent_Failed>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is ReportEvent_Failed&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode {
    return Object.hash(runtimeType,message);
}

@override
String toString() {
    return 'ReportEvent.failed(message: $message)';
}


}

/// @nodoc
abstract mixin class $ReportEvent_FailedCopyWith<$Res> implements $ReportEventCopyWith<$Res> {
  factory $ReportEvent_FailedCopyWith(ReportEvent_Failed value, $Res Function(ReportEvent_Failed) _then) = _$ReportEvent_FailedCopyWithImpl;
@useResult
$Res call({
 String message
});




}
/// @nodoc
class _$ReportEvent_FailedCopyWithImpl<$Res>
    implements $ReportEvent_FailedCopyWith<$Res> {
  _$ReportEvent_FailedCopyWithImpl(this._self, this._then);

  final ReportEvent_Failed _self;
  final $Res Function(ReportEvent_Failed) _then;

/// Create a copy of ReportEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? message = null,}) {
  return _then(ReportEvent_Failed(
message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
